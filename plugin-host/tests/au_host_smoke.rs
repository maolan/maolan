//! End-to-end smoke test for AUv2 hosting: spawns `maolan-plugin-host au`
//! against the Apple AUDelay component, then drives parameter enumeration,
//! audio blocks, a DAW-side parameter set, and a state save/restore
//! roundtrip over the real shared-memory protocol.

#![cfg(target_os = "macos")]

use maolan_plugin_protocol::protocol::*;
use maolan_plugin_protocol::ringbuf::RingBuffer;
use maolan_plugin_protocol::shm::ShmMapping;
use std::io::Read;
use std::sync::atomic::Ordering;
use std::time::Duration;

const SPEC: &str = "au:aufx:dely:appl";
const MUSIC_SPEC: &str = "au:aumu:dls :appl";
const BLOCK: usize = 256;

fn spawn_host(
    shm_name: &str,
    spec: &str,
    events: &mut maolan_plugin_protocol::events::EventPair,
) -> std::process::Child {
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_maolan-plugin-host"))
        .args([
            "au",
            spec,
            shm_name,
            "test-instance",
            &events.host_read_fd().to_string(),
            &events.host_write_fd().to_string(),
            "48000",
            "256",
            "2",
            "2",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn plugin host");
    events.close_daw_unused();
    child
}

fn setup_shm(name: &str) -> ShmMapping {
    let mapping = ShmMapping::create(name, SHM_SIZE).expect("create shm");
    unsafe {
        init_shm_layout(mapping.as_ptr(), SHM_SIZE);
        header_mut(mapping.as_ptr()).set_block_response_eventless(true);
    }
    mapping
}

fn run_block(
    events: &maolan_plugin_protocol::events::EventPair,
    header: &ShmHeader,
    child: &mut std::process::Child,
) {
    header.block_size.store(BLOCK as u32, Ordering::Release);
    header.num_input_channels.store(2, Ordering::Release);
    header.num_output_channels.store(2, Ordering::Release);
    let before = header.block_response_count();
    events.signal_host().expect("signal host");
    let start = std::time::Instant::now();
    while header.block_response_count() == before {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "host did not complete the audio block; stderr: {}",
            drain_stderr(child)
        );
        if let Some(status) = child.try_wait().expect("poll host") {
            panic!(
                "host exited during block processing: {status}; stderr: {}",
                drain_stderr(child)
            );
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn spin_request_done(header: &ShmHeader) -> bool {
    let start = std::time::Instant::now();
    while header.request_type.load(Ordering::Acquire) != 0 {
        if header.request_status.load(Ordering::Acquire) == 2 {
            return false;
        }
        if start.elapsed() > Duration::from_secs(5) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    header.request_status.load(Ordering::Acquire) == 1
}

fn drain_stderr(child: &mut std::process::Child) -> String {
    let mut buf = String::new();
    if let Some(mut err) = child.stderr.take()
        && let Ok(read) = err.read_to_string(&mut buf)
    {
        let _ = read;
    }
    buf
}

#[test]
fn au_music_device_accepts_midi() {
    let shm_name = format!("/maolan-au-midi-test-{}", std::process::id());
    let mapping = setup_shm(&shm_name);
    let ptr = mapping.as_ptr();
    let mut events = maolan_plugin_protocol::events::EventPair::new().expect("event pair");
    let mut child = spawn_host(&shm_name, MUSIC_SPEC, &mut events);

    let header = unsafe { header_ref(ptr) };
    assert!(
        wait_for_ready(header, Duration::from_secs(10)),
        "host did not become ready; stderr: {}",
        drain_stderr(&mut child)
    );
    assert_eq!(header.midi_in_port_count.load(Ordering::Acquire), 1);

    // Note-on for C4 on channel 1, then a block.
    let midi_ring = unsafe {
        let buf = midi_in_ring_ptr(ptr, 0);
        let (w, r) = midi_in_indices(ptr, 0);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    assert!(midi_ring.push(MidiEvent {
        sample_offset: 0,
        data: [0x90, 60, 100],
        channel: 0,
        flags: 0,
        _pad: 0,
    }));
    run_block(&events, header, &mut child);
    let out = unsafe { std::slice::from_raw_parts(audio_channel_ptr(ptr, 0, 1), BLOCK) };
    assert!(out.iter().all(|v| v.is_finite()), "non-finite output");

    header.shutdown_request.store(1, Ordering::Release);
    let _ = events.signal_host();
    let status = child.wait().expect("host exit");
    assert!(status.success(), "host exited with {status}");
    let _ = ShmMapping::unlink(&shm_name);
}

#[test]
fn au_host_processes_block_and_requests() {
    let shm_name = format!("/maolan-au-test-{}", std::process::id());
    let mapping = setup_shm(&shm_name);
    let ptr = mapping.as_ptr();

    let mut events = maolan_plugin_protocol::events::EventPair::new().expect("event pair");
    let mut child = spawn_host(&shm_name, SPEC, &mut events);

    let header = unsafe { header_ref(ptr) };
    assert!(
        wait_for_ready(header, Duration::from_secs(10)),
        "host did not become ready; stderr: {}",
        drain_stderr(&mut child)
    );

    let name = unsafe { read_plugin_name_from_scratch(ptr) }.expect("plugin name");
    assert!(
        name.to_lowercase().contains("delay"),
        "unexpected name: {name}"
    );
    assert_eq!(header.midi_in_port_count.load(Ordering::Acquire), 0);

    // Request 13: parameter enumeration.
    header
        .request_type
        .store(REQUEST_AU_PARAMETERS, Ordering::Release);
    assert!(spin_request_done(header), "REQUEST_AU_PARAMETERS failed");
    let params = unsafe { maolan_plugin_host::au::read_au_params_from_scratch(ptr) }
        .expect("AU params in scratch");
    assert!(!params.is_empty(), "AUDelay exposes no parameters");

    // One block of silence through the effect.
    run_block(&events, header, &mut child);
    let out = unsafe { std::slice::from_raw_parts(audio_channel_ptr(ptr, 0, 1), BLOCK) };
    assert!(out.iter().all(|v| v.is_finite()), "non-finite output");

    // Set a parameter from the DAW side and confirm an echo comes back.
    let param_ring = unsafe {
        let buf = param_ring_ptr(ptr);
        let (w, r) = param_indices(ptr);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    assert!(param_ring.push(ParameterEvent {
        param_index: params[0].index,
        value: 0.25,
        sample_offset: 0,
        event_kind: PARAM_EVENT_VALUE,
    }));
    run_block(&events, header, &mut child);
    let mut echoed = false;
    let echo_ring = unsafe {
        let buf = echo_ring_ptr(ptr);
        let (w, r) = echo_indices(ptr);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    while let Some(ev) = echo_ring.pop() {
        if ev.param_index == params[0].index && (ev.value - 0.25).abs() < 1e-6 {
            echoed = true;
            break;
        }
    }
    assert!(echoed, "parameter set was not echoed back");

    // State save/restore roundtrip.
    header.request_type.store(1, Ordering::Release);
    assert!(spin_request_done(header), "state save failed");
    let saved_size = header.scratch_size.load(Ordering::Acquire);
    assert!(saved_size > 4, "empty state blob");
    let _saved =
        unsafe { std::slice::from_raw_parts(scratch_ptr(ptr), saved_size as usize) }.to_vec();
    header.request_type.store(2, Ordering::Release);
    header.scratch_size.store(saved_size, Ordering::Release);
    assert!(spin_request_done(header), "state restore failed");

    // Shutdown.
    header.shutdown_request.store(1, Ordering::Release);
    let _ = events.signal_host();
    let status = child.wait().expect("host exit");
    assert!(status.success(), "host exited with {status}");

    let _ = ShmMapping::unlink(&shm_name);
}
