use std::path::Path;

use tauri::{ipc::Channel, AppHandle, Manager};

use crate::commands::job::{
    finish_job, prepare_job, run_batch, run_sidecar, send_started,
    single_threaded_batch_concurrency, SidecarSpec,
};
use crate::types::{BatchEntry, CompressionResult, PdfOptions, PdfQuality, ProgressEvent};
use crate::validate::validate_pdf_options;

/// Ghostscript's pdfwrite device is single-threaded but memory-hungry, so a batch
/// runs a few files at once rather than one per core.
const PDF_BATCH_CONCURRENCY_CAP: usize = 3;

/// Resolve the path to the bundled Ghostscript resource directory.
/// In dev mode, it's at `src-tauri/binaries/gs-res/`.
/// In production, Tauri bundles it into the resource directory.
fn resolve_gs_resource_dir(app: &AppHandle) -> Option<String> {
    // Production: resources are bundled via Tauri's resource system
    if let Ok(resource_dir) = app.path().resource_dir() {
        let gs_res = resource_dir.join("binaries").join("gs-res");
        if gs_res.exists() {
            return Some(gs_res.to_string_lossy().to_string());
        }
    }

    // Dev mode: look relative to the Cargo manifest directory
    let dev_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join("gs-res");
    if dev_path.exists() {
        return Some(dev_path.to_string_lossy().to_string());
    }

    None
}

fn build_gs_args(
    input: &str,
    output: &str,
    options: &PdfOptions,
    resource_dir: Option<&str>,
) -> Vec<String> {
    let preset = match options.quality {
        PdfQuality::Screen => "screen",
        PdfQuality::Ebook => "ebook",
        PdfQuality::Printer => "printer",
        PdfQuality::Prepress => "prepress",
    };

    let mut args = Vec::new();

    // Point Ghostscript to its bundled init/resource files
    if let Some(res_dir) = resource_dir {
        let init_dir = format!("{}/Resource/Init", res_dir);
        let lib_dir = format!("{}/lib", res_dir);
        let resource_dir_path = format!("{}/Resource", res_dir);
        args.push(format!("-I{}", init_dir));
        args.push(format!("-I{}", lib_dir));
        args.push(format!("-I{}", resource_dir_path));
    }

    args.extend([
        "-sDEVICE=pdfwrite".to_string(),
        "-dCompatibilityLevel=1.4".to_string(),
        format!("-dPDFSETTINGS=/{}", preset),
        "-dNOPAUSE".to_string(),
        "-dBATCH".to_string(),
        "-dQUIET".to_string(),
    ]);

    if let Some(dpi) = options.dpi {
        args.push(format!("-dColorImageResolution={}", dpi));
        args.push(format!("-dGrayImageResolution={}", dpi));
        args.push(format!("-dMonoImageResolution={}", dpi));
    }

    args.push(format!("-sOutputFile={}", output));
    args.push(input.to_string());

    args
}

pub async fn compress_pdf_inner(
    app: &AppHandle,
    input: String,
    output: String,
    options: PdfOptions,
    on_progress: &Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_pdf_options(&options)?;
    tracing::info!(input = %input, quality = ?options.quality, "Starting PDF compression");

    let ctx = prepare_job(&input, &output, "pdf").await?;
    let gs_res_dir = resolve_gs_resource_dir(app);
    let args = build_gs_args(&ctx.input, &ctx.output, &options, gs_res_dir.as_deref());

    send_started(&ctx, on_progress);

    let outcome = run_sidecar(
        app,
        &ctx,
        SidecarSpec {
            sidecar: "gs",
            args: &args,
            progress: None,
            capture_stderr: true,
        },
    )
    .await?;

    let error = (outcome.exit_code != Some(0)).then(|| {
        if outcome.stderr.trim().is_empty() {
            format!("Ghostscript exited with code {:?}", outcome.exit_code)
        } else {
            outcome.stderr.trim().to_string()
        }
    });
    Ok(finish_job(
        app,
        &ctx,
        outcome.exit_code,
        outcome.duration_ms,
        error,
        on_progress,
    )
    .await)
}

#[tauri::command]
pub async fn compress_pdfs_batch(
    app: AppHandle,
    files: Vec<BatchEntry>,
    options: PdfOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let concurrency = single_threaded_batch_concurrency(PDF_BATCH_CONCURRENCY_CAP);
    Ok(
        run_batch(&app, files, concurrency, move |app, entry| {
            let options = options.clone();
            let on_progress = on_progress.clone();
            async move {
                compress_pdf_inner(&app, entry.input, entry.output, options, &on_progress).await
            }
        })
        .await,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(quality: PdfQuality, dpi: Option<u32>) -> PdfOptions {
        PdfOptions { quality, dpi }
    }

    #[test]
    fn gs_args_ebook_preset() {
        let args = build_gs_args("in.pdf", "out.pdf", &opts(PdfQuality::Ebook, None), None);
        assert!(args.contains(&"-dPDFSETTINGS=/ebook".to_string()));
        assert!(args.contains(&"-sDEVICE=pdfwrite".to_string()));
        assert!(args.contains(&"-dNOPAUSE".to_string()));
        assert!(args.contains(&"-dBATCH".to_string()));
        assert!(args.contains(&"-dQUIET".to_string()));
        assert!(args.iter().any(|a| a.contains("sOutputFile")));
    }

    #[test]
    fn gs_args_all_presets() {
        for (q, name) in [
            (PdfQuality::Screen, "screen"),
            (PdfQuality::Ebook, "ebook"),
            (PdfQuality::Printer, "printer"),
            (PdfQuality::Prepress, "prepress"),
        ] {
            let args = build_gs_args("in.pdf", "out.pdf", &opts(q, None), None);
            assert!(args.contains(&format!("-dPDFSETTINGS=/{}", name)));
        }
    }

    #[test]
    fn gs_args_with_dpi() {
        let args = build_gs_args(
            "in.pdf",
            "out.pdf",
            &opts(PdfQuality::Ebook, Some(150)),
            None,
        );
        assert!(args.contains(&"-dColorImageResolution=150".to_string()));
        assert!(args.contains(&"-dGrayImageResolution=150".to_string()));
        assert!(args.contains(&"-dMonoImageResolution=150".to_string()));
    }

    #[test]
    fn gs_args_without_dpi() {
        let args = build_gs_args("in.pdf", "out.pdf", &opts(PdfQuality::Screen, None), None);
        assert!(!args.iter().any(|a| a.contains("ImageResolution")));
    }

    #[test]
    fn gs_args_with_resource_dir() {
        let args = build_gs_args(
            "in.pdf",
            "out.pdf",
            &opts(PdfQuality::Ebook, None),
            Some("/res"),
        );
        assert!(args.contains(&"-I/res/Resource/Init".to_string()));
        assert!(args.contains(&"-I/res/lib".to_string()));
        assert!(args.contains(&"-I/res/Resource".to_string()));
    }

    #[test]
    fn gs_args_without_resource_dir() {
        let args = build_gs_args("in.pdf", "out.pdf", &opts(PdfQuality::Ebook, None), None);
        assert!(!args.iter().any(|a| a.starts_with("-I")));
    }
}
