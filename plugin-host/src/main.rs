use std::time::Duration;

fn print_usage() {
    eprintln!("Usage:");
    eprintln!(
        "  maolan-plugin-host <format> <plugin-path> <shm-name> <instance-id> <daw-to-host-fd> <host-to-daw-fd> [sample-rate buffer-size num-inputs num-outputs]"
    );
    eprintln!(
        "  maolan-plugin-host --scan --format <format> --path <plugin-path> [--output <json-path>]"
    );
    eprintln!();
    eprintln!("  For 'clap' / 'null': 6 arguments after format.");
    eprintln!(
        "  For 'vst3' / 'lv2': 10 arguments after format (includes sample-rate, buffer-size, num-inputs, num-outputs)."
    );
}

#[cfg(target_os = "linux")]
fn setup_parent_death_signal() {
    unsafe {
        let r = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0);
        if r != 0 {
            eprintln!(
                "Warning: failed to set PR_SET_PDEATHSIG: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}

#[cfg(target_os = "freebsd")]
fn setup_parent_death_signal() {
    unsafe {
        let sig: libc::c_int = libc::SIGTERM;
        let r = libc::procctl(
            libc::P_PID,
            0,
            libc::PROC_PDEATHSIG_CTL,
            &sig as *const _ as *mut libc::c_void,
        );
        if r != 0 {
            eprintln!(
                "Warning: failed to set PROC_PDEATHSIG: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
fn setup_parent_death_signal() {}

fn parse_log_level(args: &mut Vec<String>) -> Option<tracing::Level> {
    if let Some(pos) = args.iter().position(|a| a == "--log-level") {
        args.remove(pos);
        if pos < args.len() {
            let level_str = args.remove(pos);
            match level_str.as_str() {
                "none" => None,
                "info" => Some(tracing::Level::INFO),
                "warning" => Some(tracing::Level::WARN),
                "error" => Some(tracing::Level::ERROR),
                "debug" => Some(tracing::Level::DEBUG),
                other => {
                    eprintln!("Unknown log level '{}', using none", other);
                    None
                }
            }
        } else {
            eprintln!("--log-level requires a value");
            None
        }
    } else {
        None
    }
}

fn main() {
    eprintln!("[plugin-host] started");
    setup_parent_death_signal();

    let mut args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    // Scan mode: maolan-plugin-host --scan --format <format> --path <plugin-path> [--output <json>]
    if args[1] == "--scan" {
        let mut format = None;
        let mut path = None;
        let mut output = None;
        let mut i = 2;
        while i < args.len() {
            match args[i].as_str() {
                "--format" => {
                    i += 1;
                    if i < args.len() {
                        format = Some(args[i].clone());
                    }
                }
                "--path" => {
                    i += 1;
                    if i < args.len() {
                        path = Some(args[i].clone());
                    }
                }
                "--output" => {
                    i += 1;
                    if i < args.len() {
                        output = Some(args[i].clone());
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let format = format.unwrap_or_else(|| {
            eprintln!("--scan requires --format");
            print_usage();
            std::process::exit(1);
        });
        let path = path.unwrap_or_else(|| "--system".to_string());
        let code = maolan_plugin_host::scan::run_scan(&format, &path, output.as_deref());
        std::process::exit(code);
    }

    let log_level = parse_log_level(&mut args);

    let format = args[1].clone();

    // Determine expected arg count based on format.
    let expected_args = match format.as_str() {
        "clap" | "null" | "__test__" => 7,
        "vst3" | "lv2" => 11,
        _ => {
            eprintln!("Unknown format: {}", format);
            print_usage();
            std::process::exit(4);
        }
    };

    if args.len() != expected_args {
        print_usage();
        std::process::exit(1);
    }

    let plugin_spec = args[2].clone();
    let shm_name = args[3].clone();
    let instance_id = args[4].clone();

    #[cfg(unix)]
    let d2h_fd: i32 = args[5].parse().unwrap_or(-1);
    #[cfg(unix)]
    let h2d_fd: i32 = args[6].parse().unwrap_or(-1);

    // Parse VST3/LV2-specific args (only needed on Unix where VST3/LV2 hosting runs).
    #[cfg(unix)]
    let sample_rate: f64 = if args.len() > 7 {
        args[7].parse().unwrap_or(48000.0)
    } else {
        48000.0
    };
    #[cfg(unix)]
    let buffer_size: usize = if args.len() > 8 {
        args[8].parse().unwrap_or(256)
    } else {
        256
    };
    #[cfg(unix)]
    let num_inputs: usize = if args.len() > 9 {
        args[9].parse().unwrap_or(2)
    } else {
        2
    };
    #[cfg(unix)]
    let num_outputs: usize = if args.len() > 10 {
        args[10].parse().unwrap_or(2)
    } else {
        2
    };

    // For CLAP, plugin_spec may be "path#plugin_id" to select a specific plugin
    // from a factory. If no # is present, the first plugin in the factory is used.
    let (plugin_path, _plugin_id) = if format == "clap" {
        if let Some(pos) = plugin_spec.rfind("::") {
            (
                plugin_spec[..pos].to_string(),
                plugin_spec[pos + 2..].to_string(),
            )
        } else if let Some(pos) = plugin_spec.rfind('#') {
            (
                plugin_spec[..pos].to_string(),
                plugin_spec[pos + 1..].to_string(),
            )
        } else {
            (plugin_spec.clone(), String::new())
        }
    } else {
        (plugin_spec.clone(), String::new())
    };

    if let Some(level) = log_level {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_max_level(level)
            .init();
    }

    match format.as_str() {
        "vst3" => {
            #[cfg(unix)]
            {
                if d2h_fd < 0 || h2d_fd < 0 {
                    eprintln!("Invalid event pipe file descriptors");
                    std::process::exit(3);
                }
                let events =
                    unsafe { maolan_plugin_protocol::events::EventPair::from_fds(d2h_fd, h2d_fd) };
                let mapping = match maolan_plugin_protocol::shm::ShmMapping::open_existing(
                    &shm_name,
                    maolan_plugin_protocol::protocol::SHM_SIZE,
                ) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to attach to shared memory '{}': {}", shm_name, e);
                        std::process::exit(2);
                    }
                };
                {
                    let header =
                        unsafe { maolan_plugin_protocol::protocol::header_mut(mapping.as_ptr()) };
                    header.ready.store(1, std::sync::atomic::Ordering::Release);
                }
                maolan_plugin_host::vst3_lv2_host::run_vst3(
                    maolan_plugin_host::vst3_lv2_host::Vst3RunArgs {
                        plugin_path: &plugin_spec,
                        mapping,
                        events,
                        instance_id: &instance_id,
                        sample_rate,
                        buffer_size,
                        num_inputs,
                        num_outputs,
                    },
                );
                return;
            }
            #[cfg(not(unix))]
            {
                eprintln!("VST3 plugin hosting is not supported on this platform");
                std::process::exit(4);
            }
        }
        #[cfg(unix)]
        "lv2" => {
            if d2h_fd < 0 || h2d_fd < 0 {
                eprintln!("Invalid event pipe file descriptors");
                std::process::exit(3);
            }
            let events =
                unsafe { maolan_plugin_protocol::events::EventPair::from_fds(d2h_fd, h2d_fd) };
            let mapping = match maolan_plugin_protocol::shm::ShmMapping::open_existing(
                &shm_name,
                maolan_plugin_protocol::protocol::SHM_SIZE,
            ) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to attach to shared memory '{}': {}", shm_name, e);
                    std::process::exit(2);
                }
            };
            {
                let header =
                    unsafe { maolan_plugin_protocol::protocol::header_mut(mapping.as_ptr()) };
                header.ready.store(1, std::sync::atomic::Ordering::Release);
            }
            maolan_plugin_host::vst3_lv2_host::run_lv2(
                &plugin_spec,
                mapping,
                events,
                &instance_id,
                sample_rate,
                buffer_size,
            );
            return;
        }
        #[cfg(not(unix))]
        "lv2" => {
            eprintln!("LV2 is not supported on this platform");
            std::process::exit(4);
        }
        _ => {}
    }

    #[cfg(unix)]
    let events = {
        if d2h_fd < 0 || h2d_fd < 0 {
            tracing::error!("Invalid event pipe file descriptors");
            std::process::exit(3);
        }
        unsafe { maolan_plugin_host::events::EventPair::from_fds(d2h_fd, h2d_fd) }
    };
    #[cfg(windows)]
    let events = maolan_plugin_host::events::EventPair::from_names(&args[5], &args[6])
        .unwrap_or_else(|e| {
            tracing::error!("Failed to open event handles: {}", e);
            std::process::exit(3);
        });

    eprintln!("[plugin-host] attaching to shm={}", shm_name);
    let runtime = match maolan_plugin_host::host::HostRuntime::attach(
        &shm_name,
        events,
        format.clone(),
        plugin_spec.clone(),
        instance_id.clone(),
    ) {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!(
                "[plugin-host] Failed to attach to shared memory '{}': {}",
                shm_name, e
            );
            tracing::error!("Failed to attach to shared memory '{}': {}", shm_name, e);
            std::process::exit(2);
        }
    };
    eprintln!("[plugin-host] attached successfully");

    match plugin_path.as_str() {
        "__test__" => runtime.write_test_magic(),
        "__crash__" => {
            runtime.signal_ready();
            std::process::abort();
        }
        "__hang__" => {
            runtime.signal_ready();
            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        }
        _ => {}
    }

    match format.as_str() {
        "null" => {
            runtime.signal_ready();
            runtime.run_null_plugin();
        }
        #[cfg(unix)]
        "clap" => runtime.run_clap_plugin(),
        #[cfg(not(unix))]
        "clap" => {
            tracing::error!("CLAP plugin hosting is not supported on this platform");
            runtime.run_until_shutdown();
        }
        _ => {
            runtime.signal_ready();
            runtime.run_until_shutdown();
        }
    }

    runtime.shutdown();
}
