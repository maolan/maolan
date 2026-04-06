use anyhow::{Context, Result, anyhow};
use burn::prelude::Backend;
use burn_store::{BurnpackStore, ModuleStore, TensorSnapshot};
use huggingface_hub::{Repo, RepoType, api::sync::ApiBuilder};
use maolan_generate::heartmula_runtime;
use maolan_generate::{
    BackendChoice, GenerateProgress, GenerateResponseHeader, IPC_MODE_ENV, ModelChoice, help_text,
    parse_options, read_ipc_message, validate_options, write_ipc_message,
};
use std::collections::BTreeMap;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

include!(concat!(env!("OUT_DIR"), "/model_bindings.rs"));

const HEARTMULA_GENERATE_ONLY_ENV: &str = "MAOLAN_HEARTMULA_GENERATE_ONLY";
const HEARTMULA_HAPPY_NEW_YEAR_REPO_ID: &str = "maolandaw/HeartMuLa-happy-new-year-burn";
const HEARTMULA_RL_REPO_ID: &str = "maolandaw/HeartMuLa-RL-oss-3B-20260123";
const HEARTCODEC_REPO_ID: &str = "maolandaw/HeartCodec-oss-20260123-burn";
const HEARTMULA_TOKENIZER_REL: &str = "tokenizer.json";
const HEARTMULA_GEN_CONFIG_REL: &str = "gen_config.json";

macro_rules! eprintln {
    ($($arg:tt)*) => {
        if maolan_generate::stderr_logging_enabled() {
            std::eprintln!($($arg)*);
        }
    };
}

struct HeartmulaModelPaths {
    model_dir: PathBuf,
    heartcodec_model_dir: PathBuf,
    heartmula_raw_bpk: PathBuf,
    heartcodec_raw_bpk: PathBuf,
    tokenizer_json: PathBuf,
    gen_config_json: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
struct HeartmulaGenConfig {
    text_bos_id: i64,
    text_eos_id: i64,
    audio_eos_id: i64,
    empty_id: i64,
}

struct RawBurnpackSummary {
    tensor_count: usize,
}

struct HeartmulaRuntimeSummary {
    text_vocab_size: usize,
    audio_vocab_size: usize,
    hidden_size: usize,
    audio_codebook_count: usize,
    audio_head_vocab_size: usize,
    backbone_layer_count: usize,
    decoder_layer_count: usize,
    codec_condition_width: usize,
    codec_scalar_decoder_channels: usize,
}

fn main() -> Result<()> {
    if env::var_os(IPC_MODE_ENV).is_some() {
        return run_ipc();
    }

    let options = match parse_options(env::args_os()) {
        Ok(options) => options,
        Err(err) if err.to_string() == help_text() => {
            println!("{}", help_text());
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    if options.decode_only {
        return run_decode_only(&options);
    }

    if env::var_os(HEARTMULA_GENERATE_ONLY_ENV).is_none() {
        return run_heartmula_supervisor(&options);
    }

    eprintln!(
        "generate: mode=cli model={} backend={}",
        model_name(options.model),
        backend_name(options.backend),
    );

    run_heartmula_cli(&options)
}

fn vulkan_runtime_options() -> burn::backend::wgpu::RuntimeOptions {
    burn::backend::wgpu::RuntimeOptions {
        memory_config: burn::backend::wgpu::MemoryConfiguration::ExclusivePages,
        ..Default::default()
    }
}

fn should_forward_ipc_progress(last_progress: Option<f32>, progress: f32) -> bool {
    match last_progress {
        None => true,
        Some(last) => (progress - last).abs() > f32::EPSILON,
    }
}

fn release_backend_allocations<B: Backend>(device: &B::Device) -> Result<()> {
    B::sync(device)?;
    B::memory_cleanup(device);
    Ok(())
}

fn run_decode_only(options: &maolan_generate::CliOptions) -> Result<()> {
    let frames_json = options
        .frames_json
        .as_deref()
        .ok_or_else(|| anyhow!("--decode-only requires --frames-json"))?;
    run_decode_with_frames_json(options, frames_json)
}

fn run_decode_with_frames_json(
    options: &maolan_generate::CliOptions,
    frames_json: &Path,
) -> Result<()> {
    let model_paths = resolve_heartmula_model_paths(options.model_dir.as_deref(), options.model)?;
    println!(
        "decode_only_model_dir={}",
        model_paths.heartcodec_model_dir.display()
    );
    println!("decode_only_frames_json={}", frames_json.display());
    if let Some(threads) = options.decode_threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()
            .with_context(|| format!("failed to set decode thread count to {threads}"))?;
        println!("decode_only_threads={threads}");
    }
    match options.backend {
        BackendChoice::Cpu => run_decode_only_with_backend::<burn::backend::NdArray<f32>>(
            options,
            &model_paths.heartcodec_model_dir,
            frames_json,
        ),
        BackendChoice::Vulkan => {
            let device = burn::backend::wgpu::WgpuDevice::default();
            burn::backend::wgpu::init_setup::<burn::backend::wgpu::graphics::Vulkan>(
                &device,
                vulkan_runtime_options(),
            );
            run_decode_only_with_backend::<burn::backend::Wgpu<f32, i64, u32>>(
                options,
                &model_paths.heartcodec_model_dir,
                frames_json,
            )
        }
    }
}

fn run_decode_only_with_backend<B: Backend>(
    options: &maolan_generate::CliOptions,
    model_dir: &Path,
    frames_json: &Path,
) -> Result<()>
where
    B::Device: Default,
{
    heartmula_runtime::decode_frames_to_wav::<B>(
        model_dir,
        backend_name(options.backend),
        "f32",
        frames_json,
        &options.output_path,
        options.length as f32 / 1000.0,
        &Default::default(),
        options.ode_steps,
        options.decoder_seed,
    )
}

fn spawn_generate_subprocess(options: &maolan_generate::CliOptions) -> Result<()> {
    let current_exe =
        env::current_exe().context("failed to resolve current executable for generate")?;
    let mut command = Command::new(current_exe);
    command
        .env(HEARTMULA_GENERATE_ONLY_ENV, "1")
        .arg("--model")
        .arg(model_name(options.model))
        .arg("--backend")
        .arg(backend_name(options.backend))
        .arg("--output")
        .arg(&options.output_path)
        .arg("--lyrics")
        .arg(&options.prompt)
        .arg("--cfg-scale")
        .arg(options.cfg_scale.to_string())
        .arg("--topk")
        .arg(options.topk.to_string())
        .arg("--temperature")
        .arg(options.temperature.to_string())
        .arg("--length")
        .arg(options.length.to_string())
        .arg("--ode-steps")
        .arg(options.ode_steps.to_string());
    if let Some(model_dir) = &options.model_dir {
        command.arg("--model-dir").arg(model_dir);
    }
    let status = command
        .status()
        .context("failed to spawn generation subprocess")?;
    if !status.success() {
        anyhow::bail!("generation subprocess failed with status {status}");
    }
    Ok(())
}

fn run_heartmula_supervisor(options: &maolan_generate::CliOptions) -> Result<()> {
    let frames_json_path = options.output_path.with_extension("frames.json");
    eprintln!("generate: spawning HeartMuLa generation subprocess");
    spawn_generate_subprocess(options)?;
    eprintln!("generate: generation subprocess exited; starting in-process decode");
    run_decode_with_frames_json(options, &frames_json_path)
}

fn run_ipc() -> Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let options = validate_options(read_ipc_message(&mut stdin)?)?;
    eprintln!(
        "generate: mode=ipc model={} backend={}",
        model_name(options.model),
        backend_name(options.backend),
    );

    let output = match options.backend {
        BackendChoice::Cpu => {
            let device = Default::default();
            run_heartmula_ipc_with_backend::<burn::backend::NdArray<f32>>(
                &options,
                &device,
                BackendChoice::Cpu,
                &mut stdout,
            )
        }
        BackendChoice::Vulkan => {
            let device = burn::backend::wgpu::WgpuDevice::default();
            burn::backend::wgpu::init_setup::<burn::backend::wgpu::graphics::Vulkan>(
                &device,
                vulkan_runtime_options(),
            );
            run_heartmula_ipc_with_backend::<burn::backend::Wgpu<f32, i64, u32>>(
                &options,
                &device,
                BackendChoice::Vulkan,
                &mut stdout,
            )
        }
    }?;

    write_ipc_message(&mut stdout, &output.header)?;
    eprintln!(
        "generate: mode=ipc complete backend={} output={}",
        backend_name(options.backend),
        options.output_path.display()
    );
    Ok(())
}

struct GeneratedOutput {
    header: GenerateResponseHeader,
}

fn model_name(model: ModelChoice) -> &'static str {
    match model {
        ModelChoice::HappyNewYear => "happy-new-year",
        ModelChoice::Rl => "RL",
    }
}

fn heartmula_repo_id(model: ModelChoice) -> &'static str {
    match model {
        ModelChoice::HappyNewYear => HEARTMULA_HAPPY_NEW_YEAR_REPO_ID,
        ModelChoice::Rl => HEARTMULA_RL_REPO_ID,
    }
}

fn backend_name(backend: BackendChoice) -> &'static str {
    match backend {
        BackendChoice::Cpu => "cpu",
        BackendChoice::Vulkan => "vulkan",
    }
}

fn heartmula_raw_bpk_rel() -> &'static str {
    "heartmula.bpk"
}

fn heartcodec_raw_bpk_rel() -> &'static str {
    "heartcodec.bpk"
}

fn heartmula_required_relative_files() -> [&'static str; 3] {
    [
        heartmula_raw_bpk_rel(),
        HEARTMULA_TOKENIZER_REL,
        HEARTMULA_GEN_CONFIG_REL,
    ]
}

fn heartcodec_required_relative_files() -> [&'static str; 1] {
    [heartcodec_raw_bpk_rel()]
}

fn ensure_repo_snapshot_dir(repo_id: &str, required_files: &[&'static str]) -> Result<PathBuf> {
    let api = ApiBuilder::new()
        .with_progress(true)
        .build()
        .context("failed to initialize Hugging Face client")?;
    let repo = api.repo(Repo::new(repo_id.to_string(), RepoType::Model));

    let mut snapshot_dir: Option<PathBuf> = None;

    for relative_path in required_files {
        let cached_path = repo
            .get(relative_path)
            .with_context(|| format!("failed to fetch {repo_id}/{relative_path}"))?;
        let file_snapshot_dir = cached_path.parent().ok_or_else(|| {
            anyhow!(
                "cached Hugging Face file {} has no parent directory",
                cached_path.display()
            )
        })?;
        match &snapshot_dir {
            Some(existing) if existing != file_snapshot_dir => {
                anyhow::bail!(
                    "required files for {repo_id} resolved to multiple snapshot directories: {} and {}",
                    existing.display(),
                    file_snapshot_dir.display()
                );
            }
            Some(_) => {}
            None => snapshot_dir = Some(file_snapshot_dir.to_path_buf()),
        }
    }

    snapshot_dir.ok_or_else(|| anyhow!("no files resolved for {repo_id}"))
}

fn resolve_heartmula_model_paths(
    model_dir_override: Option<&Path>,
    model: ModelChoice,
) -> Result<HeartmulaModelPaths> {
    let (model_dir, heartcodec_model_dir) = if let Some(model_dir) = model_dir_override {
        (model_dir.to_path_buf(), model_dir.to_path_buf())
    } else {
        let heartmula_snapshot_dir = ensure_repo_snapshot_dir(
            heartmula_repo_id(model),
            &heartmula_required_relative_files(),
        )?;
        let heartcodec_snapshot_dir =
            ensure_repo_snapshot_dir(HEARTCODEC_REPO_ID, &heartcodec_required_relative_files())?;
        (heartmula_snapshot_dir, heartcodec_snapshot_dir)
    };

    let paths = HeartmulaModelPaths {
        model_dir: model_dir.clone(),
        heartcodec_model_dir: heartcodec_model_dir.clone(),
        heartmula_raw_bpk: model_dir.join(heartmula_raw_bpk_rel()),
        heartcodec_raw_bpk: heartcodec_model_dir.join(heartcodec_raw_bpk_rel()),
        tokenizer_json: model_dir.join(HEARTMULA_TOKENIZER_REL),
        gen_config_json: model_dir.join(HEARTMULA_GEN_CONFIG_REL),
    };
    ensure_heartmula_model_paths(&paths)?;
    Ok(paths)
}

fn ensure_heartmula_model_paths(paths: &HeartmulaModelPaths) -> Result<()> {
    let missing = [
        (heartmula_raw_bpk_rel(), &paths.heartmula_raw_bpk),
        (heartcodec_raw_bpk_rel(), &paths.heartcodec_raw_bpk),
        (HEARTMULA_TOKENIZER_REL, &paths.tokenizer_json),
        (HEARTMULA_GEN_CONFIG_REL, &paths.gen_config_json),
    ]
    .into_iter()
    .filter_map(|(label, path)| (!path.exists()).then_some(label))
    .collect::<Vec<_>>();

    if missing.is_empty() {
        return Ok(());
    }

    anyhow::bail!(
        "HeartMula assets are incomplete in {} and {}. Missing: {}",
        paths.model_dir.display(),
        paths.heartcodec_model_dir.display(),
        missing.join(", ")
    )
}

fn run_heartmula_ipc_with_backend<B: Backend>(
    options: &maolan_generate::CliOptions,
    device: &B::Device,
    backend: BackendChoice,
    stdout: &mut impl std::io::Write,
) -> Result<GeneratedOutput> {
    let model_paths = resolve_heartmula_model_paths(options.model_dir.as_deref(), options.model)?;
    let config = load_heartmula_gen_config(&model_paths.gen_config_json)?;
    let runtime_summary = inspect_heartmula_runtime(&model_paths)?;

    // Send initial progress
    let progress = GenerateProgress {
        phase: "generator".to_string(),
        progress: 0.0,
        operation: "Loading model".to_string(),
    };
    write_ipc_message(stdout, &progress)?;

    let model = heartmula_runtime::HeartmulaModel::<B>::from_burnpack(
        &model_paths.heartmula_raw_bpk,
        device,
        runtime_summary.text_vocab_size,
        runtime_summary.audio_head_vocab_size,
    )?;
    let tags = heartmula_runtime::normalize_tags(
        options
            .tags
            .as_deref()
            .unwrap_or(heartmula_runtime::default_tags()),
    );
    let lyrics = options.prompt.trim().to_lowercase();
    let lyrics_ids = heartmula_runtime::tokenize_text(&model_paths.tokenizer_json, &lyrics)?;
    let tags_ids = heartmula_runtime::tokenize_text(&model_paths.tokenizer_json, &tags)?;
    let max_audio_frames = (options.length.max(1) / 80).max(1);

    // Use Cell for interior mutability since callback runs on the same thread.
    use std::cell::Cell;
    let last_progress = Cell::new(None::<f32>);

    // Create progress callback that writes directly to stdout
    // The callback runs synchronously during generate_frames on the main thread
    let progress_callback = |phase: &str, p: f32, op: &str| {
        if should_forward_ipc_progress(last_progress.get(), p) {
            last_progress.set(Some(p));
            let progress = GenerateProgress {
                phase: phase.to_string(),
                progress: p,
                operation: op.to_string(),
            };
            let _ = write_ipc_message(stdout, &progress);
            let _ = stdout.flush();
        }
    };

    let mut generation_config = heartmula_runtime::HeartmulaGenerationConfig {
        text_bos_id: config.text_bos_id,
        text_eos_id: config.text_eos_id,
        audio_eos_id: config.audio_eos_id,
        empty_id: config.empty_id,
        lyrics_ids: &lyrics_ids,
        tags_ids: &tags_ids,
        max_audio_frames,
        temperature: options.temperature,
        topk: options.topk,
        cfg_scale: options.cfg_scale,
        progress_callback: Some(Box::new(progress_callback)),
    };

    // Run generation - progress will be reported via callback
    let frames = model.generate_frames(device, &mut generation_config)?;
    let generated_frame_count = frames.len();

    // Drop generation_config to release the stdout borrow from the callback
    std::mem::drop(generation_config);

    // Release generator-side GPU allocations before the decoder starts.
    drop(model);
    release_backend_allocations::<B>(device)?;

    // Write frames to a temp file for decoding (HeartCodec expects a file)
    let temp_dir = std::env::temp_dir();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let frames_json_path = temp_dir.join(format!("maolan_ipc_frames_{}.json", unique));
    let output_wav_path = options.output_path.clone();

    heartmula_runtime::write_frames_json(&frames_json_path, &lyrics, &tags, &frames)?;

    // Report decoder phase start
    let progress = GenerateProgress {
        phase: "decoder".to_string(),
        progress: 0.0,
        operation: "Decoding audio".to_string(),
    };
    write_ipc_message(stdout, &progress)?;

    // Decode frames to WAV file
    heartmula_runtime::decode_frames_to_wav::<B>(
        &model_paths.heartcodec_model_dir,
        backend_name(backend),
        "f32",
        &frames_json_path,
        &output_wav_path,
        options.length as f32 / 1000.0,
        device,
        options.ode_steps,
        options.decoder_seed,
    )?;

    // Report decoder complete
    let progress = GenerateProgress {
        phase: "decoder".to_string(),
        progress: 0.99,
        operation: "Finalizing".to_string(),
    };
    write_ipc_message(stdout, &progress)?;

    // Read the WAV file into memory
    // Clean up temp files
    let _ = std::fs::remove_file(&frames_json_path);

    // Sync backend to ensure all pending GPU operations complete before process exit
    eprintln!("generate: syncing backend before exit...");
    release_backend_allocations::<B>(device)?;

    eprintln!(
        "generate: ipc backend={} frames={} output={}",
        backend_name(backend),
        generated_frame_count,
        output_wav_path.display()
    );

    let header = GenerateResponseHeader {
        backend,
        channels: 1,
        frames: generated_frame_count,
        guidance_scale: options.cfg_scale,
        prompt_tokens: lyrics_ids.len() as i64,
        sample_rate_hz: 48_000,
        length: options.length,
        steps: options.ode_steps,
    };

    Ok(GeneratedOutput { header })
}

fn inspect_heartmula_cli(options: &maolan_generate::CliOptions) -> Result<()> {
    let model_paths = resolve_heartmula_model_paths(options.model_dir.as_deref(), options.model)?;
    let config = load_heartmula_gen_config(&model_paths.gen_config_json)?;
    let heartmula_summary =
        summarize_burnpack(&model_paths.heartmula_raw_bpk, heartmula_required_tensors())?;
    let heartcodec_summary = summarize_burnpack(
        &model_paths.heartcodec_raw_bpk,
        heartcodec_required_tensors(),
    )?;
    let runtime_summary = inspect_heartmula_runtime(&model_paths)?;
    println!("generate");
    println!("model=heartmula");
    println!("model_dir={}", model_paths.model_dir.display());
    println!(
        "heartmula_raw_bpk={}",
        model_paths.heartmula_raw_bpk.display()
    );
    println!(
        "heartcodec_raw_bpk={}",
        model_paths.heartcodec_raw_bpk.display()
    );
    println!("tokenizer_json={}", model_paths.tokenizer_json.display());
    println!("gen_config_json={}", model_paths.gen_config_json.display());
    println!("heartmula_tensor_count={}", heartmula_summary.tensor_count);
    println!(
        "heartcodec_tensor_count={}",
        heartcodec_summary.tensor_count
    );
    println!("text_bos_id={}", config.text_bos_id);
    println!("text_eos_id={}", config.text_eos_id);
    println!("audio_eos_id={}", config.audio_eos_id);
    println!("empty_id={}", config.empty_id);
    println!("text_vocab_size={}", runtime_summary.text_vocab_size);
    println!("audio_vocab_size={}", runtime_summary.audio_vocab_size);
    println!("hidden_size={}", runtime_summary.hidden_size);
    println!(
        "audio_codebook_count={}",
        runtime_summary.audio_codebook_count
    );
    println!(
        "audio_head_vocab_size={}",
        runtime_summary.audio_head_vocab_size
    );
    println!(
        "backbone_layer_count={}",
        runtime_summary.backbone_layer_count
    );
    println!(
        "decoder_layer_count={}",
        runtime_summary.decoder_layer_count
    );
    println!(
        "codec_condition_width={}",
        runtime_summary.codec_condition_width
    );
    println!(
        "codec_scalar_decoder_channels={}",
        runtime_summary.codec_scalar_decoder_channels
    );
    println!("inspect_only=true");
    Ok(())
}

fn load_heartmula_gen_config(path: &Path) -> Result<HeartmulaGenConfig> {
    serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

fn summarize_burnpack(path: &Path, required_tensors: &[&str]) -> Result<RawBurnpackSummary> {
    let snapshots = load_burnpack_snapshots(path)?;
    ensure_required_tensors_exist(path, &snapshots, required_tensors)?;

    Ok(RawBurnpackSummary {
        tensor_count: snapshots.len(),
    })
}

fn load_burnpack_snapshots(path: &Path) -> Result<BTreeMap<String, TensorSnapshot>> {
    let mut store = BurnpackStore::from_file(path).zero_copy(true);
    store
        .get_all_snapshots()
        .with_context(|| format!("failed to read snapshots from {}", path.display()))
        .cloned()
}

fn ensure_required_tensors_exist(
    path: &Path,
    snapshots: &BTreeMap<String, TensorSnapshot>,
    required_tensors: &[&str],
) -> Result<()> {
    let missing = required_tensors
        .iter()
        .copied()
        .filter(|name| !snapshots.contains_key(*name))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        anyhow::bail!(
            "burnpack {} is missing required tensors: {}",
            path.display(),
            missing.join(", ")
        );
    }

    Ok(())
}

fn load_selected_tensor_shapes(
    path: &Path,
    snapshots: &BTreeMap<String, TensorSnapshot>,
    tensor_names: &[&str],
) -> Result<BTreeMap<String, Vec<usize>>> {
    let mut shapes = BTreeMap::new();

    for tensor_name in tensor_names {
        let snapshot = snapshots.get(*tensor_name).ok_or_else(|| {
            anyhow!(
                "burnpack {} is missing required tensor {}",
                path.display(),
                tensor_name
            )
        })?;
        let data = snapshot.to_data().with_context(|| {
            format!(
                "failed to materialize tensor {} from {}",
                tensor_name,
                path.display()
            )
        })?;
        shapes.insert((*tensor_name).to_string(), data.shape);
    }

    Ok(shapes)
}

fn inspect_heartmula_runtime(model_paths: &HeartmulaModelPaths) -> Result<HeartmulaRuntimeSummary> {
    let heartmula_snapshots = load_burnpack_snapshots(&model_paths.heartmula_raw_bpk)?;
    ensure_required_tensors_exist(
        &model_paths.heartmula_raw_bpk,
        &heartmula_snapshots,
        heartmula_required_tensors(),
    )?;
    let heartcodec_snapshots = load_burnpack_snapshots(&model_paths.heartcodec_raw_bpk)?;

    ensure_required_tensors_exist(
        &model_paths.heartcodec_raw_bpk,
        &heartcodec_snapshots,
        heartcodec_required_tensors(),
    )?;

    let heartmula_shapes = load_selected_tensor_shapes(
        &model_paths.heartmula_raw_bpk,
        &heartmula_snapshots,
        &[
            "text_embeddings.weight",
            "audio_embeddings.weight",
            "audio_head",
        ],
    )?;
    let heartcodec_shapes = load_selected_tensor_shapes(
        &model_paths.heartcodec_raw_bpk,
        &heartcodec_snapshots,
        &[
            "flow_matching.cond_feature_emb.weight",
            "scalar_model.decoder.0.bias",
        ],
    )?;

    let heartmula_keys = heartmula_snapshots.keys().cloned().collect::<Vec<_>>();
    let heartcodec_keys = heartcodec_snapshots.keys().cloned().collect::<Vec<_>>();

    infer_heartmula_runtime_summary(
        &heartmula_keys,
        &heartmula_shapes,
        &heartcodec_keys,
        &heartcodec_shapes,
    )
}

fn infer_heartmula_runtime_summary(
    heartmula_keys: &[String],
    heartmula_shapes: &BTreeMap<String, Vec<usize>>,
    _heartcodec_keys: &[String],
    heartcodec_shapes: &BTreeMap<String, Vec<usize>>,
) -> Result<HeartmulaRuntimeSummary> {
    let text_embeddings = expect_rank(
        heartmula_shapes,
        "text_embeddings.weight",
        2,
        "HeartMula text embeddings",
    )?;
    let audio_embeddings = expect_rank(
        heartmula_shapes,
        "audio_embeddings.weight",
        2,
        "HeartMula audio embeddings",
    )?;
    let audio_head = expect_rank(heartmula_shapes, "audio_head", 3, "HeartMula audio head")?;
    let codec_condition = expect_rank(
        heartcodec_shapes,
        "flow_matching.cond_feature_emb.weight",
        2,
        "HeartCodec condition embedding",
    )?;
    let codec_scalar_decoder = expect_rank(
        heartcodec_shapes,
        "scalar_model.decoder.0.bias",
        1,
        "HeartCodec scalar decoder bias",
    )?;

    Ok(HeartmulaRuntimeSummary {
        text_vocab_size: text_embeddings[0],
        audio_vocab_size: audio_embeddings[0],
        hidden_size: text_embeddings[1],
        audio_codebook_count: audio_head[0],
        audio_head_vocab_size: audio_head[2],
        backbone_layer_count: count_numbered_layers(
            heartmula_keys,
            "backbone_layers_",
            "_attn_q_proj_weight",
        ),
        decoder_layer_count: count_numbered_layers(
            heartmula_keys,
            "decoder_layers_",
            "_attn_q_proj_weight",
        ),
        codec_condition_width: codec_condition[1],
        codec_scalar_decoder_channels: codec_scalar_decoder[0],
    })
}

fn expect_rank<'a>(
    shapes: &'a BTreeMap<String, Vec<usize>>,
    tensor_name: &str,
    expected_rank: usize,
    label: &str,
) -> Result<&'a [usize]> {
    let shape = shapes
        .get(tensor_name)
        .ok_or_else(|| anyhow!("{label} tensor {tensor_name} is missing"))?;
    if shape.len() != expected_rank {
        anyhow::bail!(
            "{label} tensor {tensor_name} has rank {}, expected {}",
            shape.len(),
            expected_rank
        );
    }

    Ok(shape.as_slice())
}

fn count_numbered_layers(keys: &[String], prefix: &str, suffix: &str) -> usize {
    keys.iter()
        .filter_map(|key| {
            let rest = key.strip_prefix(prefix)?;
            let index = rest.strip_suffix(suffix)?;
            index.parse::<usize>().ok()
        })
        .max()
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn heartmula_required_tensors() -> &'static [&'static str] {
    &[
        "text_embeddings.weight",
        "audio_embeddings.weight",
        "projection.weight",
        "codebook0_head.weight",
        "audio_head",
        "muq_linear.weight",
        "backbone.norm.scale",
        "decoder.norm.scale",
    ]
}

fn heartcodec_required_tensors() -> &'static [&'static str] {
    &[
        "flow_matching.cond_feature_emb.weight",
        "flow_matching.estimator.proj_in.ffn_1.weight",
        "flow_matching.estimator.proj_out.ffn_2.weight",
        "scalar_model.decoder.0.bias",
    ]
}

fn run_heartmula_cli(options: &maolan_generate::CliOptions) -> Result<()> {
    if options.inspect_only {
        return inspect_heartmula_cli(options);
    }
    match options.backend {
        BackendChoice::Cpu => run_heartmula_cpu(options),
        BackendChoice::Vulkan => run_heartmula_vulkan(options),
    }
}

fn run_heartmula_with_backend<B: Backend>(
    options: &maolan_generate::CliOptions,
    backend: BackendChoice,
) -> Result<()>
where
    B::Device: Default,
{
    let model_paths = resolve_heartmula_model_paths(options.model_dir.as_deref(), options.model)?;
    let config = load_heartmula_gen_config(&model_paths.gen_config_json)?;
    let runtime_summary = inspect_heartmula_runtime(&model_paths)?;
    let heartmula_summary =
        summarize_burnpack(&model_paths.heartmula_raw_bpk, heartmula_required_tensors())?;
    let heartcodec_summary = summarize_burnpack(
        &model_paths.heartcodec_raw_bpk,
        heartcodec_required_tensors(),
    )?;
    let device = Default::default();
    let model = heartmula_runtime::HeartmulaModel::<B>::from_burnpack(
        &model_paths.heartmula_raw_bpk,
        &device,
        runtime_summary.text_vocab_size,
        runtime_summary.audio_head_vocab_size,
    )?;
    let tags = heartmula_runtime::normalize_tags(
        options
            .tags
            .as_deref()
            .unwrap_or(heartmula_runtime::default_tags()),
    );
    let lyrics = options.prompt.trim().to_lowercase();
    let lyrics_ids = heartmula_runtime::tokenize_text(&model_paths.tokenizer_json, &lyrics)?;
    let tags_ids = heartmula_runtime::tokenize_text(&model_paths.tokenizer_json, &tags)?;
    let max_audio_frames = (options.length.max(1) / 80).max(1);
    let mut generation_config = heartmula_runtime::HeartmulaGenerationConfig {
        text_bos_id: config.text_bos_id,
        text_eos_id: config.text_eos_id,
        audio_eos_id: config.audio_eos_id,
        empty_id: config.empty_id,
        lyrics_ids: &lyrics_ids,
        tags_ids: &tags_ids,
        max_audio_frames,
        temperature: options.temperature,
        topk: options.topk,
        cfg_scale: options.cfg_scale,
        progress_callback: None,
    };
    let frames = model.generate_frames(&device, &mut generation_config)?;
    let generated_frame_count = frames.len();
    let frames_json_path = options.output_path.with_extension("frames.json");
    heartmula_runtime::write_frames_json(&frames_json_path, &lyrics, &tags, &frames)?;
    println!("frames_predecode={}", frames_json_path.display());
    if env::var_os(HEARTMULA_GENERATE_ONLY_ENV).is_some() {
        eprintln!("generate: unloading HeartMuLa before process exit");
        drop(model);
        release_backend_allocations::<B>(&device)?;
        println!("generate");
        println!("model=heartmula");
        println!("mode=tokens");
        println!("model_dir={}", model_paths.model_dir.display());
        println!("backend={}", backend_name(backend));
        println!("cfg_scale={}", options.cfg_scale);
        println!("generated_frame_count={}", generated_frame_count);
        println!("frames_json={}", frames_json_path.display());
        println!("runtime=heartmula-burn");
        println!("note=HeartMuLa token generation completed in a dedicated subprocess");
        return Ok(());
    }
    drop(model);
    release_backend_allocations::<B>(&device)?;
    heartmula_runtime::decode_frames_to_wav::<B>(
        &model_paths.heartcodec_model_dir,
        backend_name(backend),
        "f32",
        &frames_json_path,
        &options.output_path,
        options.length as f32 / 1000.0,
        &device,
        options.ode_steps,
        options.decoder_seed,
    )?;
    println!("generate");
    println!("model=heartmula");
    println!("mode=tokens");
    println!("model_dir={}", model_paths.model_dir.display());
    println!("backend={}", backend_name(backend));
    println!("cfg_scale={}", options.cfg_scale);
    println!("ode_steps={}", options.ode_steps);
    println!("heartmula_tensor_count={}", heartmula_summary.tensor_count);
    println!(
        "heartcodec_tensor_count={}",
        heartcodec_summary.tensor_count
    );
    println!("text_bos_id={}", config.text_bos_id);
    println!("text_eos_id={}", config.text_eos_id);
    println!("audio_eos_id={}", config.audio_eos_id);
    println!("empty_id={}", config.empty_id);
    println!("text_vocab_size={}", runtime_summary.text_vocab_size);
    println!("audio_vocab_size={}", runtime_summary.audio_vocab_size);
    println!("hidden_size={}", runtime_summary.hidden_size);
    println!(
        "audio_codebook_count={}",
        runtime_summary.audio_codebook_count
    );
    println!(
        "audio_head_vocab_size={}",
        runtime_summary.audio_head_vocab_size
    );
    println!(
        "backbone_layer_count={}",
        runtime_summary.backbone_layer_count
    );
    println!(
        "decoder_layer_count={}",
        runtime_summary.decoder_layer_count
    );
    println!(
        "codec_condition_width={}",
        runtime_summary.codec_condition_width
    );
    println!(
        "codec_scalar_decoder_channels={}",
        runtime_summary.codec_scalar_decoder_channels
    );
    println!("generated_frame_count={}", generated_frame_count);
    println!("frames_json={}", frames_json_path.display());
    println!("output_wav={}", options.output_path.display());
    println!("runtime=heartmula-burn");
    println!("note=Burn token generation succeeded; HeartCodec detokenization raised wav output");
    Ok(())
}

fn run_heartmula_cpu(options: &maolan_generate::CliOptions) -> Result<()> {
    run_heartmula_with_backend::<burn::backend::NdArray<f32>>(options, BackendChoice::Cpu)
}

fn run_heartmula_vulkan(options: &maolan_generate::CliOptions) -> Result<()> {
    let device = burn::backend::wgpu::WgpuDevice::default();
    burn::backend::wgpu::init_setup::<burn::backend::wgpu::graphics::Vulkan>(
        &device,
        vulkan_runtime_options(),
    );
    run_heartmula_with_backend::<burn::backend::Wgpu<f32, i64, u32>>(options, BackendChoice::Vulkan)
}

#[cfg(test)]
mod tests {
    use super::{
        HEARTMULA_GEN_CONFIG_REL, HEARTMULA_TOKENIZER_REL, HeartmulaModelPaths,
        count_numbered_layers, ensure_heartmula_model_paths, heartcodec_raw_bpk_rel,
        heartmula_raw_bpk_rel, infer_heartmula_runtime_summary, should_forward_ipc_progress,
    };
    use crate::heartmula_runtime::normalize_tags;
    use std::{collections::BTreeMap, env, fs};

    #[test]
    fn normalize_tags_wraps_and_lowercases() {
        assert_eq!(normalize_tags("Piano,HAPPY"), "<tag>piano,happy</tag>");
    }

    #[test]
    fn normalize_tags_preserves_existing_wrappers() {
        assert_eq!(normalize_tags("<tag>piano</tag>"), "<tag>piano</tag>");
    }

    #[test]
    fn normalize_tags_removes_spaces_after_commas() {
        assert_eq!(normalize_tags("Piano, Happy"), "<tag>piano,happy</tag>");
        assert_eq!(normalize_tags("a,  b,   c"), "<tag>a,b,c</tag>");
    }

    #[test]
    fn ensure_heartmula_model_paths_reports_missing_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = env::temp_dir().join(format!("heartmula-assets-test-{unique}"));
        fs::create_dir_all(root.join("burn_raw")).expect("create dir");
        fs::write(root.join(HEARTMULA_TOKENIZER_REL), []).expect("tokenizer");

        let paths = HeartmulaModelPaths {
            model_dir: root.clone(),
            heartcodec_model_dir: root.clone(),
            heartmula_raw_bpk: root.join(heartmula_raw_bpk_rel()),
            heartcodec_raw_bpk: root.join(heartcodec_raw_bpk_rel()),
            tokenizer_json: root.join(HEARTMULA_TOKENIZER_REL),
            gen_config_json: root.join(HEARTMULA_GEN_CONFIG_REL),
        };
        let err = ensure_heartmula_model_paths(&paths).expect_err("missing files should error");
        let message = err.to_string();
        assert!(message.contains(heartmula_raw_bpk_rel()));
        assert!(message.contains(heartcodec_raw_bpk_rel()));
        assert!(message.contains(HEARTMULA_GEN_CONFIG_REL));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn count_numbered_layers_uses_highest_index_plus_one() {
        let keys = vec![
            "backbone_layers_0_attn_q_proj_weight".to_string(),
            "backbone_layers_2_attn_q_proj_weight".to_string(),
            "backbone_layers_1_attn_q_proj_weight".to_string(),
            "decoder_layers_0_attn_q_proj_weight".to_string(),
        ];
        assert_eq!(
            count_numbered_layers(&keys, "backbone_layers_", "_attn_q_proj_weight"),
            3
        );
        assert_eq!(
            count_numbered_layers(&keys, "decoder_layers_", "_attn_q_proj_weight"),
            1
        );
    }

    #[test]
    fn infer_heartmula_runtime_summary_reports_exported_dimensions() {
        let heartmula_keys = vec![
            "backbone_layers_0_attn_q_proj_weight".to_string(),
            "backbone_layers_1_attn_q_proj_weight".to_string(),
            "decoder_layers_0_attn_q_proj_weight".to_string(),
        ];
        let mut heartmula_shapes = BTreeMap::new();
        heartmula_shapes.insert("text_embeddings.weight".to_string(), vec![128_256, 3_072]);
        heartmula_shapes.insert("audio_embeddings.weight".to_string(), vec![65_576, 3_072]);
        heartmula_shapes.insert("audio_head".to_string(), vec![7, 3_072, 8_197]);

        let heartcodec_keys = vec!["flow_matching.cond_feature_emb.weight".to_string()];
        let mut heartcodec_shapes = BTreeMap::new();
        heartcodec_shapes.insert(
            "flow_matching.cond_feature_emb.weight".to_string(),
            vec![512, 512],
        );
        heartcodec_shapes.insert("scalar_model.decoder.0.bias".to_string(), vec![64]);

        let summary = infer_heartmula_runtime_summary(
            &heartmula_keys,
            &heartmula_shapes,
            &heartcodec_keys,
            &heartcodec_shapes,
        )
        .expect("summary");

        assert_eq!(summary.text_vocab_size, 128_256);
        assert_eq!(summary.audio_vocab_size, 65_576);
        assert_eq!(summary.hidden_size, 3_072);
        assert_eq!(summary.audio_codebook_count, 7);
        assert_eq!(summary.audio_head_vocab_size, 8_197);
        assert_eq!(summary.backbone_layer_count, 2);
        assert_eq!(summary.decoder_layer_count, 1);
        assert_eq!(summary.codec_condition_width, 512);
        assert_eq!(summary.codec_scalar_decoder_channels, 64);
    }

    #[test]
    fn ipc_progress_forwarding_emits_each_new_chunk_progress() {
        assert!(should_forward_ipc_progress(None, 0.0));
        assert!(should_forward_ipc_progress(Some(0.0), 0.08));
        assert!(should_forward_ipc_progress(Some(0.08), 0.16));
        assert!(!should_forward_ipc_progress(Some(0.16), 0.16));
    }
}
