fn main() {
    #[cfg(target_os = "openbsd")]
    {}

    if std::env::var_os("CARGO_CFG_UNIX").is_some() {
        build_test_passthrough_plugin();
    }

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        println!("cargo:rerun-if-changed=images/maolan-icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("images/maolan-icon.ico");
        if let Err(e) = res.compile() {
            eprintln!("Failed to compile Windows icon resource: {e}");
        }
    }
}

#[cfg(unix)]
fn build_test_passthrough_plugin() {
    use std::path::PathBuf;

    let source = PathBuf::from("plugin-host/tests/test_passthrough.c");
    println!("cargo:rerun-if-changed={}", source.display());

    let Some(out_dir) = std::env::var_os("OUT_DIR").map(PathBuf::from) else {
        return;
    };
    let output = out_dir.join("test_passthrough.so");
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let mut cmd = std::process::Command::new(compiler);
    if target_os == "macos" {
        cmd.arg("-dynamiclib");
    } else {
        cmd.arg("-shared");
    }
    cmd.arg("-fPIC").arg(&source).arg("-o").arg(&output);

    match cmd.status() {
        Ok(status) if status.success() => {
            println!(
                "cargo:rustc-env=MAOLAN_TEST_PASSTHROUGH_CLAP={}",
                output.display()
            );
        }
        Ok(status) => {
            panic!(
                "failed to build {}: compiler exited with {status}",
                source.display()
            );
        }
        Err(e) => {
            panic!("failed to run C compiler for {}: {e}", source.display());
        }
    }
}

#[cfg(not(unix))]
fn build_test_passthrough_plugin() {}
