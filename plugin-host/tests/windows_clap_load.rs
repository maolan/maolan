#![cfg(all(test, windows))]

use maolan_plugin_protocol::events::EventPair;
use maolan_plugin_protocol::protocol::*;
use maolan_plugin_protocol::shm::ShmMapping;
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[test]
fn surge_xt_load_and_process() {
    let surge_path = r"C:\Program Files\Common Files\CLAP\Surge Synth Team\Surge XT.clap";
    if !std::path::Path::new(surge_path).exists() {
        return;
    }

    let plugin_spec = format!("{surge_path}::org.surge-synth-team.surge-xt");
    let instance_id = "test-surge-xt-windows";
    let pid = std::process::id();
    let shm_name = format!("/maolan-{pid}-{instance_id}");

    let mapping = ShmMapping::create(&shm_name, SHM_SIZE).expect("create shm");
    unsafe {
        init_shm_layout(mapping.as_ptr(), mapping.size());
    }

    let mut events = EventPair::new().expect("create event pair");

    let host_bin = std::env::var("CARGO_BIN_EXE_maolan-plugin-host")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join("maolan-plugin-host.exe")
        });

    let mut cmd = Command::new(&host_bin);
    cmd.arg("clap")
        .arg(&plugin_spec)
        .arg(&shm_name)
        .arg(instance_id)
        .arg(events.daw_to_host_name())
        .arg(events.host_to_daw_name())
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().expect("spawn plugin host");
    events.close_daw_unused();

    let header = unsafe { header_ref(mapping.as_ptr()) };
    let start = Instant::now();
    let ready = loop {
        if header.ready.load(Ordering::Acquire) != 0 {
            break true;
        }
        if start.elapsed() >= Duration::from_secs(15) {
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(ready, "plugin host did not signal ready");

    let name = unsafe {
        let mut n = None;
        for _ in 0..50 {
            n = read_plugin_name_from_scratch(mapping.as_ptr());
            if n.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        n
    };
    println!("plugin name: {name:?}");

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
            unsafe {
                *plane.add(s) = (s as f32) / (block_size as f32);
            }
        }
    }

    events.signal_host().expect("signal host");
    events
        .wait_host(Duration::from_secs(10))
        .expect("host should complete");

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

    header.shutdown_request.store(1, Ordering::Release);
    let _ = events.signal_host();

    let _ = child.wait();
    let _ = ShmMapping::unlink(&shm_name);
}
