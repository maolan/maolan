use anyhow::{Context, Result, anyhow, bail};
use burn::prelude::Backend;
use maolan_generate::BackendChoice;
use maolan_generate::heartmula_runtime;
use serde::Deserialize;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const DEFAULT_HEARTMULA_MODEL_REPO_DIR: &str =
    "repos/heartmula-burn/artifacts/heartmula-happy-new-year-20260123";

#[derive(Debug, Clone)]
struct Options {
    backend: BackendChoice,
    model_dir: Option<PathBuf>,
    output_frames: PathBuf,
    lyrics: String,
    tags: String,
    length: i64,
    topk: usize,
    temperature: f32,
    cfg_scale: f32,
}

#[derive(Debug, Deserialize)]
struct HeartmulaGenConfig {
    text_bos_id: i64,
    text_eos_id: i64,
    audio_eos_id: i64,
    empty_id: i64,
}

fn help_text() -> &'static str {
    "\
heartmula_generate_frames

Usage:
  cargo run --release -p maolan-generate --bin heartmula_generate_frames -- [options]

Options:
  --backend <cpu|vulkan|cuda>  CUDA requires the `cuda` feature
  --model-dir <path>
  --output-frames <path>
  --lyrics <text>
  --tags <text>
  --length <int>
  --topk <int>
  --temperature <float>
  --cfg-scale <float>
  -h, --help
"
}

fn parse_options(args: impl IntoIterator<Item = OsString>) -> Result<Options> {
    let mut args = args.into_iter();
    let _program = args.next();

    let mut backend = BackendChoice::Cpu;
    let mut model_dir = None;
    let mut output_frames = PathBuf::from("heartmula.frames.json");
    let mut lyrics = None;
    let mut tags = Some(heartmula_runtime::default_tags().to_string());
    let mut length = 2000_i64;
    let mut topk = 50_usize;
    let mut temperature = 1.0_f32;
    let mut cfg_scale = 6.0_f32;

    while let Some(arg) = args.next() {
        let arg = arg
            .into_string()
            .map_err(|_| anyhow!("arguments must be valid UTF-8"))?;
        if matches!(arg.as_str(), "-h" | "--help") {
            bail!(help_text());
        }
        match arg.as_str() {
            "--backend" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value after --backend"))?
                    .into_string()
                    .map_err(|_| anyhow!("backend value must be valid UTF-8"))?;
                backend = match value.as_str() {
                    "cpu" => BackendChoice::Cpu,
                    "vulkan" => BackendChoice::Vulkan,
                    "cuda" => BackendChoice::Cuda,
                    _ => bail!("unsupported backend '{value}', expected cpu, vulkan, or cuda"),
                };
            }
            "--model-dir" => {
                model_dir = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow!("missing value after --model-dir"))?,
                ));
            }
            "--output-frames" => {
                output_frames = PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow!("missing value after --output-frames"))?,
                );
            }
            "--lyrics" => {
                lyrics = Some(
                    args.next()
                        .ok_or_else(|| anyhow!("missing value after --lyrics"))?
                        .into_string()
                        .map_err(|_| anyhow!("lyrics value must be valid UTF-8"))?,
                );
            }
            "--tags" => {
                tags = Some(
                    args.next()
                        .ok_or_else(|| anyhow!("missing value after --tags"))?
                        .into_string()
                        .map_err(|_| anyhow!("tags value must be valid UTF-8"))?,
                );
            }
            "--length" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value after --length"))?
                    .into_string()
                    .map_err(|_| anyhow!("length value must be valid UTF-8"))?;
                length = value
                    .parse::<i64>()
                    .map_err(|_| anyhow!("length must be a whole number"))?;
            }
            "--topk" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value after --topk"))?
                    .into_string()
                    .map_err(|_| anyhow!("topk value must be valid UTF-8"))?;
                topk = value
                    .parse::<usize>()
                    .map_err(|_| anyhow!("topk must be a whole number"))?;
            }
            "--temperature" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value after --temperature"))?
                    .into_string()
                    .map_err(|_| anyhow!("temperature value must be valid UTF-8"))?;
                temperature = value
                    .parse::<f32>()
                    .map_err(|_| anyhow!("temperature must be a number"))?;
            }
            "--cfg-scale" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value after --cfg-scale"))?
                    .into_string()
                    .map_err(|_| anyhow!("cfg-scale value must be valid UTF-8"))?;
                cfg_scale = value
                    .parse::<f32>()
                    .map_err(|_| anyhow!("cfg-scale must be a number"))?;
            }
            _ => bail!("unexpected argument '{arg}'"),
        }
    }

    let lyrics = lyrics
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("--lyrics is required"))?;
    let tags = heartmula_runtime::normalize_tags(tags.as_deref().unwrap_or_default());
    Ok(Options {
        backend,
        model_dir,
        output_frames,
        lyrics,
        tags,
        length,
        topk,
        temperature,
        cfg_scale,
    })
}

fn resolve_model_dir(override_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = override_dir.map(Path::to_path_buf) {
        return Ok(path);
    }
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    Ok(PathBuf::from(home).join(DEFAULT_HEARTMULA_MODEL_REPO_DIR))
}

fn heartmula_raw_bpk_rel() -> &'static str {
    "heartmula.bpk"
}

fn load_gen_config(path: &Path) -> Result<HeartmulaGenConfig> {
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn cuda_feature_error() -> anyhow::Error {
    anyhow!("CUDA backend requested, but this build was compiled without the `cuda` feature")
}

fn run_with_backend<B: Backend>(options: &Options) -> Result<()>
where
    B::Device: Default,
{
    let model_dir = resolve_model_dir(options.model_dir.as_deref())?;
    let heartmula_raw_bpk = model_dir.join(heartmula_raw_bpk_rel());
    let tokenizer_json = model_dir.join("tokenizer.json");
    let gen_config_json = model_dir.join("gen_config.json");
    let device = Default::default();
    let config = load_gen_config(&gen_config_json)?;
    let lyrics_ids = heartmula_runtime::tokenize_text(&tokenizer_json, &options.lyrics)?;
    let tags_ids = heartmula_runtime::tokenize_text(&tokenizer_json, &options.tags)?;
    let model = heartmula_runtime::HeartmulaModel::<B>::from_burnpack(
        &heartmula_raw_bpk,
        &device,
        128_256,
        8_197,
    )?;
    let generation_config = heartmula_runtime::HeartmulaGenerationConfig {
        text_bos_id: config.text_bos_id,
        text_eos_id: config.text_eos_id,
        audio_eos_id: config.audio_eos_id,
        empty_id: config.empty_id,
        lyrics_ids: &lyrics_ids,
        tags_ids: &tags_ids,
        max_audio_frames: ((options.length.max(1) as usize) / 80).max(1),
        temperature: options.temperature,
        topk: options.topk,
        cfg_scale: options.cfg_scale,
    };
    let frames = model.generate_frames(&device, &generation_config)?;
    heartmula_runtime::write_frames_json(
        &options.output_frames,
        &options.lyrics,
        &options.tags,
        &frames,
    )?;
    println!("frames_json={}", options.output_frames.display());
    println!("generated_frame_count={}", frames.len());
    Ok(())
}

fn main() -> Result<()> {
    let options = match parse_options(env::args_os()) {
        Ok(options) => options,
        Err(err) if err.to_string() == help_text() => {
            println!("{}", help_text());
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    match options.backend {
        BackendChoice::Cpu => run_with_backend::<burn::backend::NdArray<f32>>(&options),
        BackendChoice::Vulkan => {
            let device = burn::backend::wgpu::WgpuDevice::default();
            burn::backend::wgpu::init_setup::<burn::backend::wgpu::graphics::Vulkan>(
                &device,
                Default::default(),
            );
            run_with_backend::<burn::backend::Wgpu<f32, i64, u32>>(&options)
        }
        BackendChoice::Cuda => {
            #[cfg(feature = "cuda")]
            {
                run_with_backend::<burn::backend::Cuda<f32, i64>>(&options)
            }
            #[cfg(not(feature = "cuda"))]
            {
                Err(cuda_feature_error())
            }
        }
    }
}
