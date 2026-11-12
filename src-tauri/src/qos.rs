//! Keep encode work off Windows' "background" scheduling class.
//!
//! Windows 11 treats a process with no focused window as background work and, on a
//! hybrid CPU, runs it mostly on the efficiency cores. Sidecars are windowless, so
//! they are always in that class; the app itself is whenever the user switches
//! windows. Opting out of execution-speed power throttling lets them use the
//! performance cores (libx264 on a 1440p clip: ~110 fps -> ~150 fps on an i9-13900K).
//!
//! Failure is logged and ignored: this only affects speed, never correctness.
//! On other platforms both functions do nothing.

/// Opt the app's own process out of power throttling (covers the native image
/// encoders, which run in-process).
pub fn opt_out_current_process() {
    #[cfg(windows)]
    win::opt_out_current();
}

/// Opt a child process (by PID) out of power throttling. Call right after spawn.
pub fn opt_out_child(pid: u32) {
    #[cfg(windows)]
    win::opt_out_pid(pid);
    #[cfg(not(windows))]
    let _ = pid;
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    // Declared by hand rather than pulling in `windows-sys` as a direct dependency
    // for four kernel32 functions. kernel32 is always linked on Windows.
    type Handle = *mut c_void;
    type Bool = i32;

    const PROCESS_POWER_THROTTLING: i32 = 4; // PROCESS_INFORMATION_CLASS::ProcessPowerThrottling
    const PROCESS_POWER_THROTTLING_CURRENT_VERSION: u32 = 1;
    const PROCESS_POWER_THROTTLING_EXECUTION_SPEED: u32 = 0x1;
    const PROCESS_SET_INFORMATION: u32 = 0x0200;

    #[repr(C)]
    struct ProcessPowerThrottlingState {
        version: u32,
        control_mask: u32,
        state_mask: u32,
    }

    extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn OpenProcess(desired_access: u32, inherit_handle: Bool, process_id: u32) -> Handle;
        fn SetProcessInformation(
            process: Handle,
            info_class: i32,
            info: *const c_void,
            info_size: u32,
        ) -> Bool;
        fn CloseHandle(handle: Handle) -> Bool;
    }

    /// ControlMask = EXECUTION_SPEED with StateMask = 0 means "this process manages
    /// execution speed itself and wants it off", i.e. never throttle.
    fn disable_throttling(process: Handle) -> std::io::Result<()> {
        let state = ProcessPowerThrottlingState {
            version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            control_mask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            state_mask: 0,
        };
        // SAFETY: `state` is a valid, correctly sized PROCESS_POWER_THROTTLING_STATE
        // that outlives the call; `process` is a valid handle owned by the caller.
        let ok = unsafe {
            SetProcessInformation(
                process,
                PROCESS_POWER_THROTTLING,
                &state as *const _ as *const c_void,
                std::mem::size_of::<ProcessPowerThrottlingState>() as u32,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn opt_out_current() {
        // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
        let process = unsafe { GetCurrentProcess() };
        match disable_throttling(process) {
            Ok(()) => tracing::info!("Opted app process out of power throttling"),
            Err(e) => {
                tracing::warn!(error = %e, "Could not opt app process out of power throttling")
            }
        }
    }

    pub fn opt_out_pid(pid: u32) {
        // SAFETY: plain Win32 call; a null return is handled below.
        let process = unsafe { OpenProcess(PROCESS_SET_INFORMATION, 0, pid) };
        if process.is_null() {
            let e = std::io::Error::last_os_error();
            tracing::warn!(pid, error = %e, "Could not open child to opt out of power throttling");
            return;
        }
        let result = disable_throttling(process);
        // SAFETY: `process` is a real handle we opened above and close exactly once.
        unsafe { CloseHandle(process) };
        match result {
            Ok(()) => tracing::debug!(pid, "Opted child out of power throttling"),
            Err(e) => {
                tracing::warn!(pid, error = %e, "Could not opt child out of power throttling")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mechanism check: FFmpeg libx264 on the 1440p sample with and without the
    /// child opt-out. Needs the bundled sidecar and sample clip, so it is ignored by
    /// default. Run with:
    /// `QOS_CLIP=<clip> QOS_FFMPEG=<ffmpeg.exe> cargo test --lib qos -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn ffmpeg_child_opt_out_timing() {
        let clip = std::env::var("QOS_CLIP").expect("set QOS_CLIP");
        let ffmpeg = std::env::var("QOS_FFMPEG").expect("set QOS_FFMPEG");
        let out = std::env::temp_dir().join("qos_probe_out.mp4");
        let runs: usize = std::env::var("QOS_RUNS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2);
        for round in 0..runs {
            for opt_out in [false, true] {
                let start = std::time::Instant::now();
                let mut child = std::process::Command::new(&ffmpeg)
                    .args(["-nostdin", "-y", "-v", "error", "-i", &clip, "-t", "20"])
                    .args(["-c:v", "libx264", "-preset", "fast", "-pix_fmt", "yuv420p"])
                    .args(["-crf", "23", "-an"])
                    .arg(&out)
                    .spawn()
                    .expect("spawn ffmpeg");
                if opt_out {
                    opt_out_child(child.id());
                }
                let (control, state) = read_throttling_state(child.id());
                println!("  child throttling state: ControlMask={control:#x} StateMask={state:#x}");
                if opt_out {
                    assert_eq!((control, state), (1, 0), "opt-out did not take effect");
                }
                assert!(child.wait().expect("wait").success());
                let secs = start.elapsed().as_secs_f64();
                println!(
                    "round {round} opt_out={opt_out}: {secs:.2} s, {:.1} fps (1200 frames)",
                    1200.0 / secs
                );
            }
        }
        let _ = std::fs::remove_file(out);
    }

    /// Read back a process's power-throttling state: (ControlMask, StateMask).
    #[cfg(windows)]
    fn read_throttling_state(pid: u32) -> (u32, u32) {
        use std::ffi::c_void;
        extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
            fn GetProcessInformation(
                process: *mut c_void,
                class: i32,
                info: *mut c_void,
                size: u32,
            ) -> i32;
            fn CloseHandle(handle: *mut c_void) -> i32;
        }
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        let mut buf = [1u32, 0, 0]; // Version, ControlMask, StateMask
                                    // SAFETY: test-only Win32 calls; buffer matches PROCESS_POWER_THROTTLING_STATE.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            assert!(!h.is_null(), "OpenProcess for query failed");
            let ok = GetProcessInformation(h, 4, buf.as_mut_ptr() as *mut c_void, 12);
            CloseHandle(h);
            assert!(ok != 0, "GetProcessInformation failed");
        }
        (buf[1], buf[2])
    }

    #[test]
    fn opt_out_never_panics() {
        opt_out_current_process();
        // A PID that cannot exist: must log and return, not panic.
        opt_out_child(u32::MAX);
    }
}
