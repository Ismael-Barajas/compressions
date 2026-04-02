use std::time::Instant;
use uuid::Uuid;

use crate::compression::image as img_compress;
use crate::types::{BatchEntry, CompressionResult, ImageOptions};

#[tauri::command]
pub async fn compress_image(
    input: String,
    output: String,
    options: ImageOptions,
) -> Result<CompressionResult, String> {
    let job_id = Uuid::new_v4().to_string();

    let input_size = std::fs::metadata(&input)
        .map(|m| m.len())
        .unwrap_or(0);

    let start = Instant::now();

    // Run compression in a blocking task since image processing is CPU-bound
    let input_clone = input.clone();
    let output_clone = output.clone();
    let result = tokio::task::spawn_blocking(move || {
        img_compress::compress(&input_clone, &output_clone, &options)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

    match result {
        Ok(()) => {
            let output_size = std::fs::metadata(&output)
                .map(|m| m.len())
                .unwrap_or(0);

            Ok(CompressionResult {
                job_id,
                input_path: input,
                output_path: output,
                input_size,
                output_size,
                duration_ms: start.elapsed().as_millis() as u64,
                success: true,
                error: None,
            })
        }
        Err(e) => Ok(CompressionResult {
            job_id,
            input_path: input,
            output_path: output.clone(),
            input_size,
            output_size: 0,
            duration_ms: start.elapsed().as_millis() as u64,
            success: false,
            error: Some(e),
        }),
    }
}

#[tauri::command]
pub async fn compress_images_batch(
    files: Vec<BatchEntry>,
    options: ImageOptions,
) -> Result<Vec<CompressionResult>, String> {
    let mut handles = Vec::new();

    for entry in files {
        let opts = options.clone();
        let handle = tokio::spawn(async move {
            compress_image(entry.input, entry.output, opts).await
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(format!("Task join error: {}", e)),
        }
    }

    Ok(results)
}
