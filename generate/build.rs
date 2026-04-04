use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");

    let source_dir = resolve_generated_source_dir()?;
    let out_dir = PathBuf::from(env::var("OUT_DIR").context("OUT_DIR is not set")?);

    // No stable audio models - HeartMula is handled separately
    let bindings = format!(
        "pub const GENERATED_SOURCE_DIR: &str = {generated_source_dir:?};\n",
        generated_source_dir = source_dir.display().to_string(),
    );

    fs::write(out_dir.join("model_bindings.rs"), bindings)
        .context("failed to write model bindings")?;
    Ok(())
}

fn resolve_generated_source_dir() -> Result<PathBuf> {
    Ok(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("generated"))
}
