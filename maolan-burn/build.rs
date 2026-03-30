use anyhow::{Context, Result, bail};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=generated/burn_t5/stable_audio_t5_sim.rs");
    println!("cargo:rerun-if-changed=generated/burn_dit/stable_audio_dit.rs");
    println!("cargo:rerun-if-changed=generated/burn_vae/stable_audio_vae_decoder_sim.rs");

    let source_dir = resolve_generated_source_dir()?;
    let t5_rs = source_dir.join("burn_t5/stable_audio_t5_sim.rs");
    let dit_rs = source_dir.join("burn_dit/stable_audio_dit.rs");
    let vae_rs = source_dir.join("burn_vae/stable_audio_vae_decoder_sim.rs");

    for path in [&t5_rs, &dit_rs, &vae_rs] {
        if !path.exists() {
            bail!(
                "required generated model source is missing: {}",
                path.display()
            );
        }
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").context("OUT_DIR is not set")?);
    let patched_t5_rs = patch_generated_source(&t5_rs, &out_dir, "stable_audio_t5_sim.rs")?;
    let patched_dit_rs = patch_generated_source(&dit_rs, &out_dir, "stable_audio_dit.rs")?;
    let patched_vae_rs =
        patch_generated_source(&vae_rs, &out_dir, "stable_audio_vae_decoder_sim.rs")?;

    let bindings = format!(
        "pub const GENERATED_SOURCE_DIR: &str = {generated_source_dir:?};\n\
         #[allow(clippy::all)]\n\
         pub mod stable_audio_t5 {{ include!({t5_rs:?}); }}\n\
         #[allow(clippy::all)]\n\
         pub mod stable_audio_dit {{ include!({dit_rs:?}); }}\n\
         #[allow(clippy::all)]\n\
         pub mod stable_audio_vae {{ include!({vae_rs:?}); }}\n",
        generated_source_dir = source_dir.display().to_string(),
        t5_rs = patched_t5_rs.display().to_string(),
        dit_rs = patched_dit_rs.display().to_string(),
        vae_rs = patched_vae_rs.display().to_string(),
    );

    fs::write(out_dir.join("model_bindings.rs"), bindings)
        .context("failed to write model bindings")?;
    Ok(())
}

fn patch_generated_source(source: &Path, out_dir: &Path, file_name: &str) -> Result<PathBuf> {
    let text = fs::read_to_string(source)
        .with_context(|| format!("failed to read generated model source {}", source.display()))?;
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();

    for index in 0..lines.len() {
        if !lines[index].contains("::from_data(") {
            continue;
        }

        for lookahead in (index + 1)..usize::min(index + 8, lines.len()) {
            if lines[lookahead].contains("(&*self.device, burn::tensor::DType::") {
                lines[index] = lines[index].replace("::from_data(", "::from_data_dtype(");
                lines[lookahead] = lines[lookahead]
                    .replace("(&*self.device, ", "&*self.device, ")
                    .replace("),", ",");
                break;
            }
        }
    }

    let mut text = lines.join("\n");
    text = text.replace(
        "burn::tensor::TensorData::from([constant34_out1])",
        "constant34_out1.to_data()",
    );
    text = text.replace(
        "burn::tensor::TensorData::from([constant33_out1])",
        "constant33_out1.to_data()",
    );
    text = text.replace(
        "let log1_out1 = div1_out1.log();",
        "let log1_out1 = div1_out1.clamp_min(1.0f32).log();",
    );
    text = text.replace("constant31_out1 as f64", "f64::from(constant31_out1)");
    text = text.replace("alloc::vec![", "vec![");
    text = text.replace("alloc::vec::Vec", "std::vec::Vec");
    text = text.replace(
        "submodule1: Submodule1<B>,",
        "pub submodule1: Submodule1<B>,",
    );
    text = text.replace(
        "submodule2: Submodule2<B>,",
        "pub submodule2: Submodule2<B>,",
    );
    text = text.replace(
        "submodule3: Submodule3<B>,",
        "pub submodule3: Submodule3<B>,",
    );
    text = text.replace(
        "submodule4: Submodule4<B>,",
        "pub submodule4: Submodule4<B>,",
    );

    let patched = out_dir.join(file_name);
    fs::write(&patched, text)
        .with_context(|| format!("failed to write patched source {}", patched.display()))?;

    Ok(patched)
}

fn resolve_generated_source_dir() -> Result<PathBuf> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR")).join("generated"))
}
