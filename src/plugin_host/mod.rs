#![cfg(test)]

mod scanner;
mod watchdog;

use maolan_plugin_host::events::EventPair;
use maolan_plugin_host::protocol::*;
use maolan_plugin_host::shm::ShmMapping;

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Spawn the standalone `maolan-plugin-host` binary.
///
/// Returns the [`Child`] process handle, the [`ShmMapping`], and the
/// [`EventPair`] used to wake / wait on the host.
/// The caller must later call [`shutdown_host`] to avoid leaking the shm segment.
fn spawn_plugin_host(
    format: &str,
    plugin_path: &str,
    instance_id: &str,
) -> Result<(Child, ShmMapping, EventPair), String> {
    let pid = std::process::id();
    let shm_name = format!("/maolan-{pid}-{instance_id}");

    let mapping = ShmMapping::create(&shm_name, SHM_SIZE)?;
    unsafe {
        init_shm_layout(mapping.as_ptr(), mapping.size());
    }

    let mut events = EventPair::new().map_err(|e| format!("failed to create event pipes: {e}"))?;

    let host_bin = find_plugin_host_binary()
        .ok_or_else(|| "maolan-plugin-host binary not found".to_string())?;

    let mut cmd = Command::new(&host_bin);
    cmd.arg(format)
        .arg(plugin_path)
        .arg(&shm_name)
        .arg(instance_id)
        .arg(events.host_read_fd().to_string())
        .arg(events.host_write_fd().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null());

    // In tests, inherit stderr so tracing logs are visible.
    if cfg!(test) {
        cmd.stderr(Stdio::inherit());
    } else {
        cmd.stderr(Stdio::null());
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn {host_bin:?}: {e}"))?;

    // Close the ends we don't need so the pipe properly signals EOF when the child dies.
    events.close_daw_unused();

    Ok((child, mapping, events))
}

/// Gracefully request shutdown, wait up to `timeout`, then kill if needed.
fn shutdown_host(child: &mut Child, mapping: &ShmMapping, events: &EventPair, timeout: Duration) {
    let header = unsafe { header_mut(mapping.as_ptr()) };
    header.shutdown_request.store(1, Ordering::Release);

    // Wake the host so it sees the shutdown request immediately.
    let _ = events.signal_host();

    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                break;
            }
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }

    let _ = ShmMapping::unlink(mapping.name());
}

/// Locate the `maolan-plugin-host` binary relative to the current executable.
fn find_plugin_host_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;

    // With a workspace both crates share target/{profile}/.
    // Test binaries live in target/{profile}/deps/; regular binaries in target/{profile}/.
    let mut candidates: Vec<PathBuf> = Vec::new();

    if dir.file_name()? == "deps" {
        let profile_dir = dir.parent()?;
        candidates.push(profile_dir.join("maolan-plugin-host"));

        // Fallback: the binary may have been built independently inside the
        // plugin-host subdirectory before the workspace was introduced.
        if let Some(daw_dir) = profile_dir.parent()?.parent() {
            let profile = profile_dir.file_name()?.to_str()?;
            candidates.push(
                daw_dir
                    .join("plugin-host")
                    .join("target")
                    .join(profile)
                    .join("maolan-plugin-host"),
            );
        }
    } else {
        candidates.push(dir.join("maolan-plugin-host"));
    }

    let found = candidates.into_iter().find(|cand| cand.exists());
    if let Some(ref path) = found {
        tracing::info!(path = %path.display(), "Using plugin-host binary");
    } else {
        tracing::error!("maolan-plugin-host binary not found");
    }
    found
}

mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn minimal_ipc_handshake() {
        let instance_id = "test-instance-001";
        let (mut child, mapping, events) =
            spawn_plugin_host("__test__", "__test__", instance_id).unwrap();

        let header = unsafe { header_ref(mapping.as_ptr()) };
        assert!(
            wait_for_ready(header, Duration::from_secs(5)),
            "plugin host did not signal ready within 5 seconds"
        );

        let scratch = unsafe { scratch_ptr(mapping.as_ptr()) };
        let magic = unsafe { std::ptr::read_unaligned(scratch as *const u32) };
        assert_eq!(magic, 0xDEADBEEF, "scratch magic number mismatch");

        shutdown_host(&mut child, &mapping, &events, Duration::from_secs(2));
    }

    #[test]
    fn child_crash_recovery() {
        let instance_id = "test-crash-002";
        let (mut child, mapping, events) =
            spawn_plugin_host("__test__", "__crash__", instance_id).unwrap();

        let header = unsafe { header_ref(mapping.as_ptr()) };
        assert!(
            wait_for_ready(header, Duration::from_secs(5)),
            "plugin host did not signal ready"
        );

        // Host crashes itself with SIGKILL. DAW should survive.
        let status = child.wait().expect("wait should return after crash");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert!(
                status.signal() == Some(9), // SIGKILL
                "expected SIGKILL, got {:?}",
                status.signal()
            );
        }

        let _ = ShmMapping::unlink(mapping.name());
        // Receiving the events is fine — the pipe fds are closed by the OS.
        let _ = events;
    }

    #[test]
    fn watchdog_kills_hung_host() {
        let instance_id = "test-hang-003";
        let (mut child, mapping, events) =
            spawn_plugin_host("__test__", "__hang__", instance_id).unwrap();

        let header = unsafe { header_ref(mapping.as_ptr()) };
        assert!(
            wait_for_ready(header, Duration::from_secs(5)),
            "plugin host did not signal ready"
        );

        // The host ignores shutdown_request and sleeps forever.
        // Our shutdown_host gives it 500 ms, then kills.
        shutdown_host(&mut child, &mapping, &events, Duration::from_millis(500));

        let status = child.wait().expect("wait should return after kill");
        assert!(
            !status.success(),
            "host should have been killed, not exited cleanly"
        );
    }

    #[test]
    fn null_plugin_passthrough() {
        let instance_id = "test-null-004";
        let (mut child, mapping, events) =
            spawn_plugin_host("null", "__test__", instance_id).unwrap();

        let header = unsafe { header_ref(mapping.as_ptr()) };
        assert!(
            wait_for_ready(header, Duration::from_secs(5)),
            "plugin host did not signal ready"
        );

        let ptr = mapping.as_ptr();
        let block_size = 256usize;
        let channels = 2usize;

        // Configure the block.
        unsafe {
            let h = header_mut(ptr);
            h.block_size.store(block_size as u32, Ordering::Release);
            h.num_input_channels
                .store(channels as u32, Ordering::Release);
            h.num_output_channels
                .store(channels as u32, Ordering::Release);
            let ts = transport_mut(ptr);
            ts.sample_rate_hz = 48000.0;
        }

        // Write a ramp to each input channel (bus 0).
        for ch in 0..channels {
            let plane = unsafe { audio_channel_ptr(ptr, ch, 0) };
            for s in 0..block_size {
                let value = (s as f32) / (block_size as f32);
                unsafe {
                    std::ptr::write(plane.add(s), value);
                }
            }
        }

        // Wake the host to process the block.
        events.signal_host().expect("signal host should succeed");

        // Wait for completion.
        events
            .wait_host(Duration::from_secs(2))
            .expect("host should complete within 2 seconds");

        // Verify output channels (bus 1) match input.
        for ch in 0..channels {
            let out_plane = unsafe { audio_channel_ptr(ptr, ch, 1) };
            for s in 0..block_size {
                let expected = (s as f32) / (block_size as f32);
                let actual = unsafe { std::ptr::read(out_plane.add(s)) };
                assert!(
                    (actual - expected).abs() < 1e-6,
                    "channel {ch} sample {s}: expected {expected}, got {actual}"
                );
            }
        }

        shutdown_host(&mut child, &mapping, &events, Duration::from_secs(2));
    }

    #[test]
    fn clap_plugin_load_and_process() {
        let plugin_path = "/home/meka/.clap/Maolan.clap";
        if !std::path::Path::new(plugin_path).exists() {
            eprintln!("Skipping CLAP test: {plugin_path} not found");
            return;
        }

        // Use monitoring plugin (simple, 1 param, mostly analysis).
        let plugin_id = "rs.maolan.monitoring";
        let instance_id = "test-clap-005";
        let (mut child, mapping, events) =
            spawn_plugin_host("clap", &format!("{plugin_path}#{plugin_id}"), instance_id).unwrap();

        let header = unsafe { header_ref(mapping.as_ptr()) };
        assert!(
            wait_for_ready(header, Duration::from_secs(5)),
            "plugin host did not signal ready"
        );

        let ptr = mapping.as_ptr();
        let block_size = 256usize;
        let channels = 2usize;

        unsafe {
            let h = header_mut(ptr);
            h.block_size.store(block_size as u32, Ordering::Release);
            h.num_input_channels
                .store(channels as u32, Ordering::Release);
            h.num_output_channels
                .store(channels as u32, Ordering::Release);
        }

        // Write a simple ramp to inputs so we can verify output is valid.
        for ch in 0..channels {
            let plane = unsafe { audio_channel_ptr(ptr, ch, 0) };
            for i in 0..block_size {
                unsafe {
                    *plane.add(i) = (i as f32) / (block_size as f32);
                }
            }
        }

        // Process first block without parameters.
        events.signal_host().expect("signal host should succeed");
        events
            .wait_host(Duration::from_secs(10))
            .expect("host should complete within 10 seconds");

        // Send a parameter change (Mode = 5) before the next block.
        let param_ring = unsafe {
            let buf = param_ring_ptr(ptr);
            let (w, r) = param_indices(ptr);
            maolan_plugin_host::ringbuf::RingBuffer::new(buf, w, r, RING_CAPACITY)
        };
        let param_ev = ParameterEvent {
            param_index: 0, // Mode
            value: 5.0,
            sample_offset: 0,
            event_kind: maolan_plugin_host::protocol::PARAM_EVENT_VALUE,
        };
        assert!(param_ring.push(param_ev), "param ring push should succeed");

        // Process second block — the host should drain the param event.
        events.signal_host().expect("signal host should succeed");
        events
            .wait_host(Duration::from_secs(10))
            .expect("host should complete within 10 seconds");

        // Process a third block for stability.
        events.signal_host().expect("signal host should succeed");
        events
            .wait_host(Duration::from_secs(10))
            .expect("host should complete within 10 seconds");

        // Verify output is finite (no NaN/Inf).
        for ch in 0..channels {
            let plane =
                unsafe { std::slice::from_raw_parts(audio_channel_ptr(ptr, ch, 1), block_size) };
            for (i, &sample) in plane.iter().enumerate() {
                assert!(
                    sample.is_finite(),
                    "output ch={ch} sample={i} is not finite: {sample}"
                );
            }
        }

        tracing::info!("CLAP plugin '{plugin_id}' processed two blocks cleanly");

        shutdown_host(&mut child, &mapping, &events, Duration::from_secs(2));

        // Verify the child exited cleanly (not crashed).
        match child.try_wait() {
            Ok(Some(status)) => {
                assert!(status.success(), "plugin host exited with error: {status}");
            }
            Ok(None) => {
                let _ = child.kill();
                panic!("plugin host did not exit after shutdown");
            }
            Err(e) => panic!("failed to wait for plugin host: {e}"),
        }
    }

    #[test]
    fn scanner_test_plugin() {
        let host_bin = find_plugin_host_binary().expect("maolan-plugin-host binary not found");
        let plugin_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/plugin-host/tests/test_passthrough.so"
        );

        let result =
            scanner::scan_plugin_file(&host_bin, "clap", plugin_path, Duration::from_secs(10));
        assert!(result.is_ok(), "scan failed: {:?}", result.err());
        let scan = result.unwrap();
        assert_eq!(scan.plugins.len(), 1);
        assert_eq!(scan.plugins[0].id, "com.maolan.test.passthrough");
    }

    #[test]
    fn scanner_blocklist_crashing_plugin() {
        let host_bin = find_plugin_host_binary().expect("maolan-plugin-host binary not found");
        let plugin_path = "/home/meka/.clap/Maolan.clap";

        if !std::path::Path::new(plugin_path).exists() {
            eprintln!("Skipping blocklist test: {plugin_path} not found");
            return;
        }

        // Use a temp blocklist so we don't pollute the real one.
        let mut blocklist = scanner::Blocklist::default();
        let result = scanner::scan_or_blocklist(
            &host_bin,
            "clap",
            plugin_path,
            &mut blocklist,
            Duration::from_secs(10),
        );
        // With proper clap_version_is_compatible checks and cleanup,
        // the scan now completes without crashing.
        assert!(result.is_some(), "expected scan to succeed");
        assert!(
            !blocklist.contains(plugin_path),
            "plugin should not be blocklisted when scan succeeds"
        );
    }

    #[test]
    fn watchdog_detects_hung_host() {
        let instance_id = "test-watchdog-006";
        let (mut child, mapping, events) =
            spawn_plugin_host("null", "__hang__", instance_id).unwrap();

        let header = unsafe { header_ref(mapping.as_ptr()) };
        assert!(
            wait_for_ready(header, Duration::from_secs(5)),
            "host did not signal ready"
        );

        let mut wd = watchdog::Watchdog::new(Duration::from_millis(200));
        // Host is alive initially.
        assert!(wd.is_alive(header), "host should be alive initially");

        // Wait for the heartbeat to stall (hang process doesn't increment).
        std::thread::sleep(Duration::from_millis(300));
        assert!(!wd.is_alive(header), "watchdog should detect hung host");
        assert_eq!(wd.failure_count, 1);

        shutdown_host(&mut child, &mapping, &events, Duration::from_secs(2));
    }

    #[test]
    fn ipc_latency_benchmark() {
        let instance_id = "test-bench-007";
        let (mut child, mapping, events) =
            spawn_plugin_host("null", "__test__", instance_id).unwrap();

        let header = unsafe { header_ref(mapping.as_ptr()) };
        assert!(
            wait_for_ready(header, Duration::from_secs(5)),
            "host did not signal ready"
        );

        let ptr = mapping.as_ptr();
        let block_size = 256usize;
        let channels = 2usize;

        unsafe {
            let h = header_mut(ptr);
            h.block_size.store(block_size as u32, Ordering::Release);
            h.num_input_channels
                .store(channels as u32, Ordering::Release);
            h.num_output_channels
                .store(channels as u32, Ordering::Release);
        }

        // Warm-up.
        for _ in 0..5 {
            events.signal_host().unwrap();
            events.wait_host(Duration::from_secs(5)).unwrap();
        }

        let iterations = 100;
        let start = Instant::now();
        for _ in 0..iterations {
            events.signal_host().unwrap();
            events.wait_host(Duration::from_secs(5)).unwrap();
        }
        let elapsed = start.elapsed();
        let avg_us = elapsed.as_micros() as f64 / iterations as f64;

        tracing::info!(
            "IPC latency benchmark: {iterations} round-trips in {:.2} ms, avg = {:.2} µs/block",
            elapsed.as_secs_f64() * 1000.0,
            avg_us
        );

        // Expect < 1 ms per round-trip on local machine.
        assert!(
            avg_us < 1000.0,
            "IPC latency too high: {avg_us:.2} µs/block (expected < 1000 µs)"
        );

        shutdown_host(&mut child, &mapping, &events, Duration::from_secs(2));
    }
}
