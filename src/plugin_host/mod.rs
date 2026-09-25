#![cfg(test)]

mod scanner;
mod watchdog;

use maolan_plugin_host::events::EventPair;
use maolan_plugin_host::protocol::*;
use maolan_plugin_host::shm::ShmMapping;

use maolan_engine::plugins::ipc::hide_console_window;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Returns the real user home directory, ignoring any temporary override of
/// `HOME` that tests may have performed. The plugin-host scanner needs a
/// stable home directory to locate CLAP/VST3 plugins by ID.
#[cfg(unix)]
fn real_user_home_dir() -> Option<PathBuf> {
    use nix::unistd::{Uid, User};
    User::from_uid(Uid::current()).ok().flatten().map(|u| u.dir)
}

#[cfg(not(unix))]
fn real_user_home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

fn spawn_plugin_host(
    format: &str,
    plugin_id: &str,
    instance_id: &str,
    clap_search_root: Option<&std::path::Path>,
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
        .arg(plugin_id)
        .arg(&shm_name)
        .arg(instance_id);
    #[cfg(unix)]
    cmd.arg(events.host_read_fd().to_string())
        .arg(events.host_write_fd().to_string());
    #[cfg(windows)]
    cmd.arg(events.daw_to_host_name())
        .arg(events.host_to_daw_name());
    if let Some(home) = real_user_home_dir() {
        cmd.env("HOME", home);
    }
    if let Some(root) = clap_search_root {
        cmd.env("CLAP_PATH", root);
    }
    append_parent_log_level(&mut cmd);
    hide_console_window(&mut cmd);
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

    if let Some(candidate) = candidates.into_iter().find(|cand| cand.exists()) {
        return Some(candidate);
    }

    #[cfg(test)]
    if dir.file_name()? == "deps" {
        return build_plugin_host_binary(dir.parent()?, host_name);
    }

    None
}

#[cfg(test)]
fn build_plugin_host_binary(profile_dir: &std::path::Path, host_name: &str) -> Option<PathBuf> {
    let profile = profile_dir.file_name()?.to_str()?;
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("build")
        .arg("-p")
        .arg("maolan-plugin-host")
        .arg("--bin")
        .arg("maolan-plugin-host");

    if profile == "release" {
        cmd.arg("--release");
    }

    if let Some(target_dir) = profile_dir.parent().and_then(|target_parent| {
        let target = target_parent.file_name()?.to_str()?;
        if target.contains('-') {
            Some((target_parent.parent()?.to_path_buf(), target.to_string()))
        } else {
            None
        }
    }) {
        cmd.arg("--target")
            .arg(target_dir.1)
            .arg("--target-dir")
            .arg(target_dir.0);
    }

    let status = cmd.status().ok()?;
    if status.success() {
        let candidate = profile_dir.join(host_name);
        candidate.exists().then_some(candidate)
    } else {
        None
    }
}

mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    /// Plugin ID exercised by `clap_plugin_load_and_process`. This was
    /// formerly `rs.maolan.monitoring`; the plugin was renamed to vumeter.
    const TEST_CLAP_PLUGIN_ID: &str = "rs.maolan.vumeter";

    static UNIQUE_BUNDLE_DIR_COUNTER: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);

    /// A Maolan CLAP bundle located without depending on a user-global
    /// install, plus the directory to hand to the spawned host via CLAP_PATH.
    struct TestClapBundle {
        search_root: PathBuf,
        bundle: PathBuf,
        temp_dir: Option<PathBuf>,
    }

    impl TestClapBundle {
        fn cleanup(&self) {
            if let Some(dir) = &self.temp_dir {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    /// Returns true if `path` is a readable binary that advertises `plugin_id`.
    fn binary_contains_plugin_id(path: &Path, plugin_id: &str) -> bool {
        std::fs::read(path)
            .map(|data| {
                data.windows(plugin_id.len())
                    .any(|window| window == plugin_id.as_bytes())
            })
            .unwrap_or(false)
    }

    /// Locates (or assembles) a Maolan CLAP bundle containing `plugin_id`,
    /// without relying on `~/.clap` holding a current build:
    ///
    /// 1. `MAOLAN_TEST_CLAP_BUNDLE` env var, if set and containing the ID.
    /// 2. A temp bundle linked from the sibling `../plugins` debug cdylib.
    ///
    /// Returns `None` (caller skips) when no usable bundle exists.
    fn prepare_test_clap_bundle(plugin_id: &str) -> Option<TestClapBundle> {
        if let Ok(override_path) = std::env::var("MAOLAN_TEST_CLAP_BUNDLE") {
            let bundle = PathBuf::from(override_path);
            if binary_contains_plugin_id(&bundle, plugin_id) {
                let search_root = bundle.parent()?.to_path_buf();
                return Some(TestClapBundle {
                    search_root,
                    bundle,
                    temp_dir: None,
                });
            }
            return None;
        }

        let lib_name = if cfg!(windows) {
            "maolan_plugins.dll"
        } else if cfg!(target_os = "macos") {
            "libmaolan_plugins.dylib"
        } else {
            "libmaolan_plugins.so"
        };
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let sibling_workspace = manifest_dir.parent()?;
        let cdylib = sibling_workspace
            .join("plugins")
            .join("target")
            .join("debug")
            .join(lib_name);
        if !binary_contains_plugin_id(&cdylib, plugin_id) {
            return None;
        }

        let unique = UNIQUE_BUNDLE_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_dir =
            std::env::temp_dir().join(format!("maolan-clap-test-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).ok()?;
        let bundle = temp_dir.join("Maolan.clap");
        let linked = {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&cdylib, &bundle).is_ok()
            }
            #[cfg(not(unix))]
            {
                false
            }
        };
        if !linked && std::fs::copy(&cdylib, &bundle).is_err() {
            let _ = std::fs::remove_dir_all(&temp_dir);
            return None;
        }

        Some(TestClapBundle {
            search_root: temp_dir.clone(),
            bundle,
            temp_dir: Some(temp_dir),
        })
    }

    #[test]
    fn minimal_ipc_handshake() {
        let instance_id = "test-instance-001";
        let (mut child, mapping, events) =
            spawn_plugin_host("__test__", "__test__", instance_id, None).unwrap();

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
    fn watchdog_kills_hung_host() {
        let instance_id = "test-hang-003";
        let (mut child, mapping, events) =
            spawn_plugin_host("__test__", "__hang__", instance_id, None).unwrap();

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
            spawn_plugin_host("null", "__test__", instance_id, None).unwrap();

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
        let Some(clap_bundle) = prepare_test_clap_bundle(TEST_CLAP_PLUGIN_ID) else {
            return;
        };

        let plugin_id = TEST_CLAP_PLUGIN_ID;
        let instance_id = "test-clap-005";
        let (mut child, mapping, events) = spawn_plugin_host(
            "clap",
            plugin_id,
            instance_id,
            Some(&clap_bundle.search_root),
        )
        .unwrap();

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

        clap_bundle.cleanup();
    }

    #[test]
    fn scanner_blocklist_crashing_plugin() {
        let host_bin = find_plugin_host_binary().expect("maolan-plugin-host binary not found");
        let Some(clap_bundle) = prepare_test_clap_bundle(TEST_CLAP_PLUGIN_ID) else {
            return;
        };
        let plugin_path = clap_bundle.bundle.to_string_lossy().into_owned();

        let mut blocklist = crate::plugin_blocklist::Blocklist::default();
        let result = scanner::scan_or_blocklist(
            &host_bin,
            "clap",
            &plugin_path,
            &mut blocklist,
            Duration::from_secs(10),
        );

        clap_bundle.cleanup();

        assert!(result.is_some(), "expected scan to succeed");
        assert!(
            !blocklist.contains(&plugin_path),
            "plugin should not be blocklisted when scan succeeds"
        );
    }

    #[test]
    fn watchdog_detects_hung_host() {
        let instance_id = "test-watchdog-006";
        let (mut child, mapping, events) =
            spawn_plugin_host("null", "__hang__", instance_id, None).unwrap();

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
            spawn_plugin_host("null", "__test__", instance_id, None).unwrap();

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
