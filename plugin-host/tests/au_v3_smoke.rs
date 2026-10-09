//! AUv3 hosting tests.
//!
//! Two layers:
//! 1. In-process `V3Unit` tests over the Apple AUDelay component. AUDelay is
//!    an AUv2 component, but `+[AUAudioUnit instantiateWithComponentDescription:]`
//!    wraps v2 components in the ObjC AUAudioUnit API, so these exercise the
//!    full V3Unit code path: instantiation blocks, bus formats, render block,
//!    parameter tree, state.
//! 2. A registry-driven end-to-end test that enumerates real AUv3 components
//!    (`kAudioComponentFlag_IsV3AudioUnit`), spawns `maolan-plugin-host au`
//!    against one and pumps an audio block over SHM. This machine may have no
//!    AUv3 extensions installed; the test skips with a message in that case.

#![cfg(target_os = "macos")]

use maolan_plugin_host::au::v3::V3Unit;
use maolan_plugin_host::au::{AuComponentDesc, AuInitConfig, AuTransport, AuUnit};
use maolan_plugin_protocol::protocol::*;
use maolan_plugin_protocol::shm::ShmMapping;
use std::sync::atomic::Ordering;
use std::time::Duration;

const SPEC: &str = "au:aufx:dely:appl";
const BLOCK: usize = 256;

fn parse_spec(spec: &str) -> AuComponentDesc {
    maolan_plugin_host::au::parse_au_spec(spec)
        .expect("spec parses")
        .desc
}

fn init_unit() -> V3Unit {
    let mut unit = V3Unit::open(&parse_spec(SPEC)).expect("open V3Unit over AUDelay");
    unit.initialize(&AuInitConfig {
        sample_rate: 48_000.0,
        max_block_size: BLOCK,
        num_inputs: 2,
        num_outputs: 2,
    })
    .expect("initialize V3Unit");
    unit
}

#[test]
fn v3unit_enumerates_parameters() {
    let unit = init_unit();
    assert!(
        !unit.parameters().is_empty(),
        "AUDelay exposes no AUv3 parameters"
    );
    let p = &unit.parameters()[0];
    assert!(p.max > p.min, "invalid range for {}", p.name);
    let value = unit.get_parameter(p.index).expect("get value");
    assert!(value.is_finite(), "non-finite value for {}", p.name);
}

#[test]
fn v3unit_renders_finite_output() {
    let mut unit = init_unit();
    // Impulse on input channel 0 so the delay line has something to echo.
    // Wrapped v2 units (the only kind exercisable without an installed AUv3
    // extension) report kAudioUnitErr_NoConnection on repeated v3 renders,
    // so assert on a single block; real AUv3 extensions render continuously
    // via the pull block.
    unit.input_buffer_mut(0).expect("input 0")[0] = 1.0;
    unit.render(BLOCK, &[], &AuTransport::default())
        .expect("render block");
    for ch in 0..2 {
        let out = unit.output_buffer(ch).expect("output plane");
        assert!(
            out.iter().all(|v| v.is_finite()),
            "non-finite output on channel {ch}"
        );
    }
}

#[test]
fn v3unit_param_set_get_and_updates() {
    let mut unit = init_unit();
    let p = unit.parameters()[0].index;
    unit.set_parameter(p, 0.25).expect("set parameter");
    assert!(
        (unit.get_parameter(p).expect("get") - 0.25).abs() < 1e-4,
        "parameter did not stick"
    );
    let _ = unit.take_param_updates();
}

#[test]
fn v3unit_state_roundtrip() {
    let mut unit = init_unit();
    let p = unit.parameters()[0].index;
    unit.set_parameter(p, 0.125).expect("set parameter");
    let state = unit.save_state().expect("save state");
    assert!(state.len() > 8, "state blob too small");
    unit.set_parameter(p, 0.75).expect("change parameter");
    unit.restore_state(&state).expect("restore state");
    let value = unit.get_parameter(p).expect("get after restore");
    assert!(
        (value - 0.125).abs() < 1e-4,
        "parameter not restored: {value}"
    );
}

#[test]
fn v3unit_view_controller_request_does_not_hang() {
    // A v2-wrapped component has no AUv3 view controller; the request must
    // fail fast rather than hang or raise (ViewBridge raises an uncatchable
    // ObjC exception without a running NSApplication).
    let unit = init_unit();
    match unsafe { maolan_plugin_host::au::v3::request_view_controller(unit.raw_unit()) } {
        Ok(vc) => assert!(vc.is_null(), "wrapped AUDelay unexpectedly has a VC"),
        Err(e) => assert!(e.contains("NSApplication"), "unexpected error: {e}"),
    }
}

/// End-to-end over the real SHM protocol against a genuine AUv3 component
/// from the registry, if one is installed.
#[test]
fn au_v3_host_end_to_end_if_present() {
    let records = maolan_plugin_host::scan::scan_au_plugins();
    let Some(record) = records.iter().find(|r| r.is_v3) else {
        eprintln!(
            "SKIP: no AUv3 components installed on this system ({} AU components scanned)",
            records.len()
        );
        return;
    };
    eprintln!("exercising AUv3 component: {} ({})", record.id, record.name);

    let shm_name = format!("/maolan-auv3-test-{}", std::process::id());
    let mapping = ShmMapping::create(&shm_name, SHM_SIZE).expect("create shm");
    let ptr = mapping.as_ptr();
    unsafe {
        init_shm_layout(ptr, SHM_SIZE);
        header_mut(ptr).set_block_response_eventless(true);
    }
    let mut events = maolan_plugin_protocol::events::EventPair::new().expect("event pair");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_maolan-plugin-host"))
        .args([
            "au",
            &record.id,
            &shm_name,
            "auv3-test",
            &events.host_read_fd().to_string(),
            &events.host_write_fd().to_string(),
            "48000",
            "256",
            "1",
            "1",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn plugin host");
    events.close_daw_unused();

    let header = unsafe { header_ref(ptr) };
    let ready = wait_for_ready(header, Duration::from_secs(30));
    if !ready {
        let mut buf = String::new();
        if let Some(mut err) = child.stderr.take()
            && std::io::Read::read_to_string(&mut err, &mut buf).is_ok()
        {}
        panic!("AUv3 host did not become ready; stderr: {buf}");
    }

    header.block_size.store(BLOCK as u32, Ordering::Release);
    header.num_input_channels.store(1, Ordering::Release);
    header.num_output_channels.store(1, Ordering::Release);
    let before = header.block_response_count();
    events.signal_host().expect("signal host");
    let start = std::time::Instant::now();
    while header.block_response_count() == before {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "host did not complete the audio block"
        );
        if let Some(status) = child.try_wait().expect("poll host") {
            panic!("host exited during block processing: {status}");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let out = unsafe { std::slice::from_raw_parts(audio_channel_ptr(ptr, 0, 1), BLOCK) };
    assert!(out.iter().all(|v| v.is_finite()), "non-finite output");

    // Parameter enumeration over the scratch payload.
    header
        .request_type
        .store(REQUEST_AU_PARAMETERS, Ordering::Release);
    let start = std::time::Instant::now();
    while header.request_type.load(Ordering::Acquire) != 0 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "REQUEST_AU_PARAMETERS failed"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(header.request_status.load(Ordering::Acquire), 1);

    header.shutdown_request.store(1, Ordering::Release);
    let _ = events.signal_host();
    let status = child.wait().expect("host exit");
    assert!(status.success(), "host exited with {status}");
    let _ = ShmMapping::unlink(&shm_name);
}
