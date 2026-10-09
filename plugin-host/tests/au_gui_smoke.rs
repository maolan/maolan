#![cfg(target_os = "macos")]
use maolan_plugin_protocol::protocol::*;
use maolan_plugin_protocol::shm::ShmMapping;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[test]
fn au_gui_show_hide_with_apple_au() {
    let shm_name = format!("/maolan-au-gui-test-{}", std::process::id());
    let mapping = ShmMapping::create(&shm_name, SHM_SIZE).expect("create shm");
    let ptr = mapping.as_ptr();
    unsafe { init_shm_layout(ptr, SHM_SIZE) };
    let mut events = maolan_plugin_protocol::events::EventPair::new().unwrap();
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_maolan-plugin-host"))
        .args([
            "au",
            "au:aufx:dely:appl",
            &shm_name,
            "gui-test",
            &events.host_read_fd().to_string(),
            &events.host_write_fd().to_string(),
            "48000",
            "256",
            "2",
            "2",
            "--log-level",
            "debug",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    events.close_daw_unused();
    let header = unsafe { header_ref(ptr) };
    assert!(wait_for_ready(header, Duration::from_secs(10)));
    header.set_gui_mode(GuiMode::Floating);
    header.request_type.store(3, Ordering::Release);
    let start = std::time::Instant::now();
    while header.request_type.load(Ordering::Acquire) != 0 {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "gui_show did not complete"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    eprintln!(
        "gui_show status = {}",
        header.request_status.load(Ordering::Acquire)
    );
    if header.request_status.load(Ordering::Acquire) != 1 {
        header.shutdown_request.store(1, Ordering::Release);
        let _ = events.signal_host();
        let out = child.wait_with_output().unwrap();
        panic!("gui_show failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    std::thread::sleep(Duration::from_secs(3)); // window visible
    header.request_type.store(4, Ordering::Release);
    while header.request_type.load(Ordering::Acquire) != 0 {
        std::thread::sleep(Duration::from_millis(5));
    }
    header.shutdown_request.store(1, Ordering::Release);
    let _ = events.signal_host();
    let out = child.wait_with_output().unwrap();
    eprintln!("host stderr: {}", String::from_utf8_lossy(&out.stderr));
    let _ = ShmMapping::unlink(&shm_name);
}
