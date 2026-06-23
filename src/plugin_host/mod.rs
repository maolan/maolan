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
        .arg(instance_id);
    #[cfg(unix)]
    cmd.arg(events.host_read_fd().to_string())
        .arg(events.host_write_fd().to_string());
    #[cfg(windows)]
    cmd.arg(events.daw_to_host_name())
        .arg(events.host_to_daw_name());
    append_parent_log_level(&mut cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::null());

    if cfg!(test) {
        cmd.stderr(Stdio::inherit());
    } else {
        cmd.stderr(Stdio::null());
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn {host_bin:?}: {e}"))?;

    events.close_daw_unused();

    Ok((child, mapping, events))
}

fn append_parent_log_level(cmd: &mut Command) {
    let parent_args: Vec<String> = std::env::args().collect();
    if let Some(pos) = parent_args.iter().position(|a| a == "--log-level")
        && pos + 1 < parent_args.len()
    {
        cmd.arg("--log-level").arg(&parent_args[pos + 1]);
    }
}

fn shutdown_host(child: &mut Child, mapping: &ShmMapping, events: &EventPair, timeout: Duration) {
    let header = unsafe { header_mut(mapping.as_ptr()) };
    header.shutdown_request.store(1, Ordering::Release);

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

fn find_plugin_host_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;

    let mut candidates: Vec<PathBuf> = Vec::new();

    let host_name = if cfg!(windows) {
        "maolan-plugin-host.exe"
    } else {
        "maolan-plugin-host"
    };

    if dir.file_name()? == "deps" {
        let profile_dir = dir.parent()?;
        candidates.push(profile_dir.join(host_name));

        if let Some(daw_dir) = profile_dir.parent()?.parent() {
            let profile = profile_dir.file_name()?.to_str()?;
            candidates.push(
                daw_dir
                    .join("plugin-host")
                    .join("target")
                    .join(profile)
                    .join(host_name),
            );
        }
    } else {
        candidates.push(dir.join(host_name));
    }

    candidates.into_iter().find(|cand| cand.exists())
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

        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            let status = child.wait().expect("wait should return after crash");
            assert!(
                status.signal() == Some(9),
                "expected SIGKILL, got {:?}",
                status.signal()
            );
        }
        #[cfg(windows)]
        let _ = child.wait().expect("wait should return after crash");

        let _ = ShmMapping::unlink(mapping.name());

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

        for ch in 0..channels {
            let plane = unsafe { audio_channel_ptr(ptr, ch, 0) };
            for s in 0..block_size {
                let value = (s as f32) / (block_size as f32);
                unsafe {
                    std::ptr::write(plane.add(s), value);
                }
            }
        }

        events.signal_host().expect("signal host should succeed");

        events
            .wait_host(Duration::from_secs(2))
            .expect("host should complete within 2 seconds");

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
            return;
        }

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

        for ch in 0..channels {
            let plane = unsafe { audio_channel_ptr(ptr, ch, 0) };
            for i in 0..block_size {
                unsafe {
                    *plane.add(i) = (i as f32) / (block_size as f32);
                }
            }
        }

        events.signal_host().expect("signal host should succeed");
        events
            .wait_host(Duration::from_secs(10))
            .expect("host should complete within 10 seconds");

        let param_ring = unsafe {
            let buf = param_ring_ptr(ptr);
            let (w, r) = param_indices(ptr);
            maolan_plugin_host::ringbuf::RingBuffer::new(buf, w, r, RING_CAPACITY)
        };
        let param_ev = ParameterEvent {
            param_index: 0,
            value: 5.0,
            sample_offset: 0,
            event_kind: maolan_plugin_host::protocol::PARAM_EVENT_VALUE,
        };
        assert!(param_ring.push(param_ev), "param ring push should succeed");

        events.signal_host().expect("signal host should succeed");
        events
            .wait_host(Duration::from_secs(10))
            .expect("host should complete within 10 seconds");

        events.signal_host().expect("signal host should succeed");
        events
            .wait_host(Duration::from_secs(10))
            .expect("host should complete within 10 seconds");

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

        shutdown_host(&mut child, &mapping, &events, Duration::from_secs(2));

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
            return;
        }

        let mut blocklist = scanner::Blocklist::default();
        let result = scanner::scan_or_blocklist(
            &host_bin,
            "clap",
            plugin_path,
            &mut blocklist,
            Duration::from_secs(10),
        );

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

        assert!(wd.is_alive(header), "host should be alive initially");

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

        assert!(
            avg_us < 1000.0,
            "IPC latency too high: {avg_us:.2} µs/block (expected < 1000 µs)"
        );

        shutdown_host(&mut child, &mapping, &events, Duration::from_secs(2));
    }
}
