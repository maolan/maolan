use std::path::Path;

fn main() {
    // Tell Cargo to rebuild if maolan-generate changes
    // We need to list all files explicitly for proper tracking
    let generate_src = Path::new("generate/src");
    if generate_src.exists() {
        // Track all .rs files in generate/src
        for entry in walkdir(generate_src) {
            println!("cargo:rerun-if-changed={}", entry.display());
        }
    }
    println!("cargo:rerun-if-changed=generate/Cargo.toml");
    println!("cargo:rerun-if-changed=generate/build.rs");

    #[cfg(target_os = "openbsd")]
    {
        println!("cargo:rustc-link-search=native=/usr/X11R6/lib");
    }
}

fn walkdir(path: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if path.is_file() && path.extension().is_some_and(|e| e == "rs") {
        files.push(path.to_path_buf());
    } else if path.is_dir()
        && let Ok(entries) = std::fs::read_dir(path)
    {
        for entry in entries.flatten() {
            files.extend(walkdir(&entry.path()));
        }
    }
    files
}
