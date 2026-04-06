use std::path::Path;
use std::time::Instant;

use tauri::{ipc::Channel, AppHandle, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use uuid::Uuid;

use crate::history::storage as history;
use crate::types::{
    BatchEntry, CompressionResult, HistoryEntry, PdfOptions, PdfQuality, ProgressEvent,
};
use crate::utils::resolve_output_conflict;
use crate::validate::validate_pdf_options;

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

#[tauri::command]
pub async fn compress_pdf(
    app: AppHandle,
    input: String,
    output: String,
    options: PdfOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_pdf_options(&options)?;
    let job_id = Uuid::new_v4().to_string();
    tracing::info!(input = %input, quality = ?options.quality, "Starting PDF compression");
    let file_name = std::path::Path::new(&input)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let input_size = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);

    if let Some(parent) = std::path::Path::new(&output).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    // Avoid overwriting existing files — append _2, _3, etc.
    let output = resolve_output_conflict(&output);

    let gs_res_dir = resolve_gs_resource_dir(&app);
    let args = build_gs_args(&input, &output, &options, gs_res_dir.as_deref());

    let (mut rx, _child) = app
        .shell()
        .sidecar("gs")
        .map_err(|e| format!("Failed to create Ghostscript sidecar: {}", e))?
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to spawn Ghostscript: {}", e))?;

    let _ = on_progress.send(ProgressEvent::Started {
        job_id: job_id.clone(),
        file_name: file_name.clone(),
        input_path: input.clone(),
    });

    let start = Instant::now();
    let mut stderr_output = String::new();

    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stderr(bytes) => {
                stderr_output.push_str(&String::from_utf8_lossy(&bytes));
            }
            CommandEvent::Terminated(status) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let output_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);

                let success = status.code == Some(0);
                let result = CompressionResult {
                    job_id: job_id.clone(),
                    input_path: input.clone(),
                    output_path: output.clone(),
                    input_size,
                    output_size,
                    duration_ms,
                    success,
                    error: if success {
                        None
                    } else {
                        Some(if stderr_output.is_empty() {
                            format!("Ghostscript exited with code {:?}", status.code)
                        } else {
                            stderr_output.trim().to_string()
                        })
                    },
                };

                if success {
                    let _ = on_progress.send(ProgressEvent::Completed(result.clone()));
                } else {
                    if let Err(e) = std::fs::remove_file(&output) {
                        tracing::warn!(path = %output, error = %e, "Failed to remove failed output");
                    }
                    let _ = on_progress.send(ProgressEvent::Error {
                        job_id: job_id.clone(),
                        message: result.error.clone().unwrap_or_default(),
                    });
                }

                if result.success {
                    tracing::info!(input = %result.input_path, output_size = result.output_size, duration_ms = result.duration_ms, "PDF compression completed");
                } else {
                    tracing::warn!(input = %result.input_path, error = ?result.error, "PDF compression failed");
                }
                if let Err(e) =
                    history::append_entry(&app, HistoryEntry::from_result(&result, "pdf"))
                {
                    tracing::warn!(error = %e, "Failed to save history entry");
                }
                return Ok(result);
            }
            _ => {}
        }
    }

    Err("Ghostscript process ended unexpectedly".to_string())
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

#[tauri::command]
pub async fn compress_pdfs_batch(
    app: AppHandle,
    files: Vec<BatchEntry>,
    options: PdfOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let mut results = Vec::new();
    for entry in files {
        let result = compress_pdf(
            app.clone(),
            entry.input,
            entry.output,
            options.clone(),
            on_progress.clone(),
        )
        .await?;
        results.push(result);
    }
    Ok(results)
}
