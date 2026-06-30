#![cfg_attr(windows, windows_subsystem = "windows")]

use std::time::Duration;

fn print_usage() {}

#[cfg(target_os = "linux")]
fn setup_parent_death_signal() {
    unsafe {
        let r = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0);
        if r != 0 {}
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
        if r != 0 {}
    }
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
fn setup_parent_death_signal() {}

fn parse_log_level(args: &mut Vec<String>) -> Option<Option<tracing::Level>> {
    if let Some(pos) = args.iter().position(|a| a == "--log-level") {
        args.remove(pos);
        if pos < args.len() {
            let level_str = args.remove(pos);
            Some(match level_str.as_str() {
                "none" => None,
                "info" => Some(tracing::Level::INFO),
                "warning" => Some(tracing::Level::WARN),
                "error" => Some(tracing::Level::ERROR),
                "debug" => Some(tracing::Level::DEBUG),
                _ => return None,
            })
        } else {
            None
        }
    } else {
        None
    }
}

fn main() {
    setup_parent_death_signal();

    let mut args: Vec<String> = std::env::args().collect();
    let log_level = parse_log_level(&mut args);

    if let Some(Some(level)) = log_level {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_max_level(level)
            .init();
    }

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

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
            print_usage();
            std::process::exit(1);
        });
        let path = path.unwrap_or_else(|| "--system".to_string());
        let code = maolan_plugin_host::scan::run_scan(&format, &path, output.as_deref());
        std::process::exit(code);
    }

    let format = args[1].clone();

    let expected_args = match format.as_str() {
        "clap" | "null" | "__test__" => 7,
        "vst3" | "lv2" => 11,
        _ => {
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

    if log_level.is_none() {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    match format.as_str() {
        "vst3" => {
            #[cfg(unix)]
            {
                if d2h_fd < 0 || h2d_fd < 0 {
                    std::process::exit(3);
                }
                let events =
                    unsafe { maolan_plugin_protocol::events::EventPair::from_fds(d2h_fd, h2d_fd) };
                let mapping = match maolan_plugin_protocol::shm::ShmMapping::open_existing(
                    &shm_name,
                    maolan_plugin_protocol::protocol::SHM_SIZE,
                ) {
                    Ok(m) => m,
                    Err(_e) => {
                        std::process::exit(2);
                    }
                };
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
                std::process::exit(4);
            }
        }
        #[cfg(unix)]
        "lv2" => {
            if d2h_fd < 0 || h2d_fd < 0 {
                std::process::exit(3);
            }
            let events =
                unsafe { maolan_plugin_protocol::events::EventPair::from_fds(d2h_fd, h2d_fd) };
            let mapping = match maolan_plugin_protocol::shm::ShmMapping::open_existing(
                &shm_name,
                maolan_plugin_protocol::protocol::SHM_SIZE,
            ) {
                Ok(m) => m,
                Err(_e) => {
                    std::process::exit(2);
                }
            };
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
            std::process::exit(4);
        }
        _ => {}
    }

    #[cfg(unix)]
    let events = {
        if d2h_fd < 0 || h2d_fd < 0 {
            std::process::exit(3);
        }
        unsafe { maolan_plugin_host::events::EventPair::from_fds(d2h_fd, h2d_fd) }
    };
    #[cfg(windows)]
    let events = maolan_plugin_host::events::EventPair::from_names(&args[5], &args[6])
        .unwrap_or_else(|_e| {
            std::process::exit(3);
        });

    let runtime = match maolan_plugin_host::host::HostRuntime::attach(
        &shm_name,
        events,
        format.clone(),
        plugin_spec.clone(),
        instance_id.clone(),
    ) {
        Ok(rt) => rt,
        Err(_e) => {
            std::process::exit(2);
        }
    };

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
        "clap" => runtime.run_clap_plugin(),
        _ => {
            runtime.signal_ready();
            runtime.run_until_shutdown();
        }
    }

    runtime.shutdown();
}
