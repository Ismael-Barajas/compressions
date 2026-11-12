use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use tauri::{ipc::Channel, AppHandle, Manager, State};

use crate::commands::job::{
    ffmpeg_failure_message, finish_job, prepare_job, resolve_duration, run_batch_planned,
    run_sidecar, send_started, ProgressTarget, SidecarSpec,
};
use crate::ffmpeg::args::build_video_args;
use crate::state::{AppState, HwEncoders};
use crate::types::{BatchEntry, CompressionResult, ProgressEvent, VideoCodec, VideoOptions};
use crate::validate::validate_video_options;

/// Resolve which HW encoder (if any) to use for the given codec.
fn resolve_hw_encoder(
    codec: &VideoCodec,
    hw_encoders: &std::collections::HashSet<String>,
) -> Option<String> {
    let name = match codec {
        VideoCodec::H264 => {
            if hw_encoders.contains("h264_videotoolbox") {
                Some("h264_videotoolbox")
            } else if hw_encoders.contains("h264_nvenc") {
                Some("h264_nvenc")
            } else {
                None
            }
        }
        VideoCodec::H265 => {
            if hw_encoders.contains("hevc_videotoolbox") {
                Some("hevc_videotoolbox")
            } else if hw_encoders.contains("hevc_nvenc") {
                Some("hevc_nvenc")
            } else {
                None
            }
        }
        // NVENC only (171 fps against 67-100 for SVT-AV1 on a 1440p clip);
        // VideoToolbox has no AV1 encoder.
        VideoCodec::AV1 => hw_encoders.contains("av1_nvenc").then_some("av1_nvenc"),
    };
    name.map(|s| s.to_string())
}

fn pick_hw_encoder(app: &AppHandle, codec: &VideoCodec) -> Option<String> {
    let hw = app.try_state::<HwEncoders>()?;
    let encoders = hw.0.read().ok()?;
    resolve_hw_encoder(codec, &encoders)
}

/// NVENC encodes from this app running right now (counted only while the GPU
/// encode itself runs, not during a software retry).
static NVENC_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// Set for the rest of the session once a second NVENC session failed to open
/// while another was running: from then on NVENC encodes run one at a time.
static NVENC_ONE_AT_A_TIME: AtomicBool = AtomicBool::new(false);

/// Counts one NVENC encode in [`NVENC_IN_FLIGHT`] for as long as it is alive.
struct NvencInFlight;

impl NvencInFlight {
    fn enter() -> Self {
        NVENC_IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        NvencInFlight
    }

    /// Whether another NVENC encode from this app is running besides this one.
    fn others_running(&self) -> bool {
        NVENC_IN_FLIGHT.load(Ordering::SeqCst) > 1
    }
}

impl Drop for NvencInFlight {
    fn drop(&mut self) {
        NVENC_IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

fn is_nvenc(hw_encoder: &str) -> bool {
    hw_encoder.ends_with("_nvenc")
}

/// What a HW encode failure that software then got through on the same file says.
#[derive(Debug, PartialEq, Eq)]
enum HwFailureVerdict {
    /// The encoder does not work here: drop it for the session.
    DisableEncoder,
    /// NVENC failed while another of our NVENC encodes was running. That is more
    /// likely a session limit (an older driver's cap, another program holding
    /// sessions, VRAM) than a broken encoder, so keep it and run one at a time.
    OneNvencAtATime,
}

fn hw_failure_verdict(hw_encoder: &str, other_nvenc_was_running: bool) -> HwFailureVerdict {
    if is_nvenc(hw_encoder) && other_nvenc_was_running {
        HwFailureVerdict::OneNvencAtATime
    } else {
        HwFailureVerdict::DisableEncoder
    }
}

/// A HW encoder that failed at runtime is dropped from the detected set so the
/// remaining files in the batch go straight to software instead of paying for a
/// failed spawn each.
fn disable_hw_encoder(app: &AppHandle, name: &str) {
    if let Some(hw) = app.try_state::<HwEncoders>() {
        if let Ok(mut set) = hw.0.write() {
            if set.remove(name) {
                tracing::warn!(encoder = %name, "HW encoder failed at runtime; disabled for this session");
            }
        }
    }
}

/// Compress one video with `hw_encoder` (or software when `None`). The encoder is
/// chosen by the batch when it gives this file its slot, so the slot size and the
/// encoder always agree.
pub async fn compress_video_inner(
    app: &AppHandle,
    input: String,
    output: String,
    options: VideoOptions,
    hw_encoder: Option<String>,
    duration_hint: Option<f64>,
    on_progress: &Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_video_options(&options)?;
    tracing::info!(input = %input, output = %output, "Starting video compression");

    let ctx = prepare_job(&input, &output, "video").await?;
    let total_duration = resolve_duration(app, &input, duration_hint).await;

    send_started(&ctx, on_progress);

    let mut opts = options.clone();
    // Always overwrite: the field is backend-owned and must not be trusted from IPC.
    opts.hw_encoder = hw_encoder.clone();
    if let Some(ref hw) = hw_encoder {
        tracing::info!(encoder = %hw, "Trying HW encoder");
    }

    let args = build_video_args(&ctx.input, &ctx.output, &opts);
    let nvenc_guard = hw_encoder
        .as_deref()
        .filter(|hw| is_nvenc(hw))
        .map(|_| NvencInFlight::enter());
    let first_run = run_sidecar(
        app,
        &ctx,
        SidecarSpec {
            sidecar: "ffmpeg",
            args: &args,
            progress: Some(ProgressTarget::full(total_duration, on_progress)),
            capture_output: false,
        },
    )
    .await;
    // Read before this encode stops counting, so it sees whoever ran alongside it.
    let other_nvenc_was_running = nvenc_guard
        .as_ref()
        .is_some_and(NvencInFlight::others_running);
    drop(nvenc_guard);
    let mut outcome = first_run?;

    // HW encoder failed — retry with software (unless the user cancelled, either
    // this file or the whole queue: a killed encoder is not a broken encoder).
    if should_retry_in_software(
        outcome.exit_code,
        hw_encoder.is_some(),
        outcome.cancelled,
        crate::commands::queue::is_cancelled(app),
    ) {
        tracing::warn!("HW encoder failed, retrying this file in software");
        if let Err(e) = tokio::fs::remove_file(&ctx.output).await {
            tracing::warn!(path = %ctx.output, error = %e, "Failed to remove partial HW output");
        }
        opts.hw_encoder = None;
        let sw_args = build_video_args(&ctx.input, &ctx.output, &opts);
        outcome = run_sidecar(
            app,
            &ctx,
            SidecarSpec {
                sidecar: "ffmpeg",
                args: &sw_args,
                progress: Some(ProgressTarget::full(total_duration, on_progress)),
                capture_output: false,
            },
        )
        .await?;

        // Only blame the HW encoder when software succeeds on the same input. If both
        // fail, the file is the problem (corrupt, empty, unsupported) and the GPU
        // encoder must stay enabled for the rest of the session.
        if outcome.exit_code == Some(0) {
            if let Some(ref hw) = hw_encoder {
                match hw_failure_verdict(hw, other_nvenc_was_running) {
                    HwFailureVerdict::DisableEncoder => disable_hw_encoder(app, hw),
                    HwFailureVerdict::OneNvencAtATime => {
                        if !NVENC_ONE_AT_A_TIME.swap(true, Ordering::SeqCst) {
                            tracing::warn!(
                                encoder = %hw,
                                "NVENC failed while another NVENC encode was running; \
                                 keeping it, one NVENC encode at a time for this session"
                            );
                        }
                    }
                }
            }
        } else {
            tracing::info!(
                "Software encode failed too; the input is at fault, HW encoder stays enabled"
            );
        }
    }

    let error = (outcome.exit_code != Some(0)).then(|| ffmpeg_failure_message(outcome.exit_code));
    Ok(finish_job(
        app,
        &ctx,
        outcome.exit_code,
        outcome.duration_ms,
        error,
        outcome.cancelled,
        on_progress,
    )
    .await)
}

fn should_retry_in_software(
    exit_code: Option<i32>,
    used_hw: bool,
    job_cancelled: bool,
    all_cancelled: bool,
) -> bool {
    exit_code != Some(0) && used_hw && !job_cancelled && !all_cancelled
}

/// Permits in the video gate: an NVENC encode takes one, anything else both.
const VIDEO_PERMITS: u32 = 2;

/// How many of the [`VIDEO_PERMITS`] one video takes, given the encoder it will use.
///
/// Two NVENC encodes may run together: the encode is on the GPU, the CPU is mostly
/// idle, and two encodes measured close to double the throughput (173 -> 334 fps
/// HEVC 1440p, 273 -> 548 fps H.264 1080p on an RTX 4090); three or four gave
/// nothing more. Cards with a single encoder chip should not lose from it. Software
/// (libx264/x265/SVT-AV1) runs alone because it already fills the cores, and
/// VideoToolbox runs alone until it has been measured on a Mac. NVENC also runs
/// alone once this session has seen a second NVENC session fail to open.
fn video_permits(hw_encoder: Option<&str>, nvenc_one_at_a_time: bool) -> u32 {
    let shares_the_gate = hw_encoder.is_some_and(is_nvenc) && !nvenc_one_at_a_time;
    if shares_the_gate {
        1
    } else {
        VIDEO_PERMITS
    }
}

/// Encodes two videos at once only when both are NVENC encodes; a software or
/// VideoToolbox encode runs alone (see [`video_permits`]). The encoder and its
/// permits are decided per file at the moment it is next in line, so a GPU encoder
/// disabled mid-batch, the switch to one NVENC encode at a time, or hardware
/// detection finishing after the batch started all apply to the files after it. A
/// file waiting for its permits has not started: no Started event, no FFmpeg, and
/// Stop, Cancel All and Pause treat it like any queued file.
///
/// Not covered: the software retry of a failed NVENC encode keeps that file's one
/// permit, so it can overlap one NVENC encode (never another software encode).
#[tauri::command]
pub async fn compress_videos_batch(
    app: AppHandle,
    files: Vec<BatchEntry>,
    options: VideoOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let codec = options.codec.clone();
    let initial_encoder = pick_hw_encoder(&app, &codec);
    tracing::info!(
        files = files.len(),
        encoder = initial_encoder.as_deref().unwrap_or("software"),
        nvenc_one_at_a_time = NVENC_ONE_AT_A_TIME.load(Ordering::SeqCst),
        "Starting video batch"
    );
    let plan = move |app: &AppHandle| {
        let hw_encoder = pick_hw_encoder(app, &codec);
        let one_at_a_time = NVENC_ONE_AT_A_TIME.load(Ordering::SeqCst);
        let permits = video_permits(hw_encoder.as_deref(), one_at_a_time);
        tracing::info!(
            encoder = hw_encoder.as_deref().unwrap_or("software"),
            permits,
            "Video slot"
        );
        (permits, hw_encoder)
    };
    Ok(run_batch_planned(
        &app,
        files,
        VIDEO_PERMITS,
        plan,
        move |app, entry, hw_encoder| {
            let options = options.clone();
            let on_progress = on_progress.clone();
            async move {
                compress_video_inner(
                    &app,
                    entry.input,
                    entry.output,
                    options,
                    hw_encoder,
                    entry.duration,
                    &on_progress,
                )
                .await
            }
        },
    )
    .await)
}

#[tauri::command]
pub fn cancel_compression(
    state: State<'_, std::sync::Mutex<AppState>>,
    job_id: String,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    // Mark before killing so the runner sees "stopped by user" when the child exits
    // (and so a child that has not registered yet is killed on registration).
    app_state.cancelled_jobs.insert(job_id.clone());
    if let Some((child, _)) = app_state.active_jobs.remove(&job_id) {
        child
            .kill()
            .map_err(|e| format!("Failed to kill process: {}", e))?;
        // Partial file cleanup happens in the job runner once the process exits
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn prefers_videotoolbox_over_nvenc() {
        let hw = set(&["h264_videotoolbox", "h264_nvenc"]);
        assert_eq!(
            resolve_hw_encoder(&VideoCodec::H264, &hw).as_deref(),
            Some("h264_videotoolbox")
        );
    }

    #[test]
    fn falls_back_to_nvenc() {
        let hw = set(&["hevc_nvenc"]);
        assert_eq!(
            resolve_hw_encoder(&VideoCodec::H265, &hw).as_deref(),
            Some("hevc_nvenc")
        );
        assert_eq!(resolve_hw_encoder(&VideoCodec::H264, &hw), None);
    }

    #[test]
    fn software_retry_only_for_genuine_hw_failures() {
        // HW failed on its own: retry.
        assert!(should_retry_in_software(Some(1), true, false, false));
        // Per-file Stop killed the HW encode: no retry.
        assert!(!should_retry_in_software(None, true, true, false));
        assert!(!should_retry_in_software(Some(1), true, true, false));
        // Cancel All: no retry.
        assert!(!should_retry_in_software(Some(1), true, false, true));
        // Software encode failed: nothing to fall back to.
        assert!(!should_retry_in_software(Some(1), false, false, false));
        // Success: no retry.
        assert!(!should_retry_in_software(Some(0), true, false, false));
    }

    /// How many videos fit in the gate at once when every file takes `permits`.
    fn at_once(permits: u32) -> u32 {
        VIDEO_PERMITS / permits
    }

    #[test]
    fn two_videos_at_once_only_on_nvenc() {
        assert_eq!(at_once(video_permits(Some("h264_nvenc"), false)), 2);
        assert_eq!(at_once(video_permits(Some("hevc_nvenc"), false)), 2);
        assert_eq!(at_once(video_permits(Some("av1_nvenc"), false)), 2);
        // Not measured on a Mac: runs alone.
        assert_eq!(at_once(video_permits(Some("h264_videotoolbox"), false)), 1);
        assert_eq!(at_once(video_permits(Some("hevc_videotoolbox"), false)), 1);
        // Software encoders already fill the cores.
        assert_eq!(at_once(video_permits(None, false)), 1);
    }

    #[test]
    fn a_software_encode_never_shares_the_gate() {
        // An NVENC encode holds 1 of 2; software needs both, so it waits for the
        // NVENC encode to finish, and an NVENC file waits for a software one.
        let nvenc = video_permits(Some("hevc_nvenc"), false);
        let software = video_permits(None, false);
        assert!(nvenc + software > VIDEO_PERMITS);
        assert_eq!(nvenc + nvenc, VIDEO_PERMITS);
    }

    #[test]
    fn nvenc_runs_alone_after_a_second_session_failed() {
        for hw in ["h264_nvenc", "hevc_nvenc", "av1_nvenc"] {
            assert_eq!(video_permits(Some(hw), true), VIDEO_PERMITS, "{hw}");
        }
        assert_eq!(video_permits(None, true), VIDEO_PERMITS);
    }

    #[test]
    fn hw_failure_with_a_second_nvenc_running_keeps_the_encoder() {
        assert_eq!(
            hw_failure_verdict("hevc_nvenc", true),
            HwFailureVerdict::OneNvencAtATime
        );
        assert_eq!(
            hw_failure_verdict("av1_nvenc", true),
            HwFailureVerdict::OneNvencAtATime
        );
        // Failed on its own: the encoder is at fault.
        assert_eq!(
            hw_failure_verdict("hevc_nvenc", false),
            HwFailureVerdict::DisableEncoder
        );
        // VideoToolbox never runs two at once, so it is always disabled as before.
        assert_eq!(
            hw_failure_verdict("h264_videotoolbox", true),
            HwFailureVerdict::DisableEncoder
        );
    }

    #[test]
    fn nvenc_in_flight_counts_the_other_encodes() {
        // Other tests do not touch the counter, so it starts from zero here.
        let first = NvencInFlight::enter();
        assert!(!first.others_running());
        let second = NvencInFlight::enter();
        assert!(first.others_running());
        assert!(second.others_running());
        drop(second);
        assert!(!first.others_running());
        drop(first);
        assert_eq!(NVENC_IN_FLIGHT.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn permits_follow_resolved_encoder() {
        // A Windows/Linux box with NVENC but no AV1 encoder: H.264 and H.265 go to
        // the GPU two at a time, AV1 is software and runs alone.
        let hw = set(&["h264_nvenc", "hevc_nvenc"]);
        let for_codec =
            |c: VideoCodec| video_permits(resolve_hw_encoder(&c, &hw).as_deref(), false);
        assert_eq!(for_codec(VideoCodec::H264), 1);
        assert_eq!(for_codec(VideoCodec::H265), 1);
        assert_eq!(for_codec(VideoCodec::AV1), VIDEO_PERMITS);
        // VideoToolbox wins over NVENC when both are present, so it runs alone.
        let mac = set(&["h264_videotoolbox", "h264_nvenc"]);
        assert_eq!(
            video_permits(
                resolve_hw_encoder(&VideoCodec::H264, &mac).as_deref(),
                false
            ),
            VIDEO_PERMITS
        );
    }

    #[test]
    fn av1_uses_nvenc_only_when_the_card_has_it() {
        let older_card = set(&["h264_nvenc", "hevc_nvenc", "h264_videotoolbox"]);
        assert_eq!(resolve_hw_encoder(&VideoCodec::AV1, &older_card), None);
        let newer_card = set(&["h264_nvenc", "hevc_nvenc", "av1_nvenc"]);
        assert_eq!(
            resolve_hw_encoder(&VideoCodec::AV1, &newer_card),
            Some("av1_nvenc".to_string())
        );
        assert_eq!(video_permits(Some("av1_nvenc"), false), 1);
    }
}
