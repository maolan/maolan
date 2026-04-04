use anyhow::{Context, Result, anyhow};
use burn::prelude::Backend;
use burn_store::{BurnpackStore, ModuleStore, TensorSnapshot};
use huggingface_hub::{Repo, RepoType, api::sync::ApiBuilder};
use maolan_generate::heartmula_runtime;
use maolan_generate::{
    BackendChoice, GenerateResponseHeader, ModelChoice, help_text, parse_options, read_ipc_message,
    validate_options, write_ipc_bytes, write_ipc_message,
};
use std::collections::BTreeMap;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

include!(concat!(env!("OUT_DIR"), "/model_bindings.rs"));

const IPC_MODE_ENV: &str = "MAOLAN_BURN_SOCKETPAIR";
const HEARTMULA_GENERATE_ONLY_ENV: &str = "MAOLAN_HEARTMULA_GENERATE_ONLY";
const HEARTMULA_REPO_ID: &str = "maolandaw/HeartMuLa-happy-new-year-burn";
const HEARTCODEC_REPO_ID: &str = "maolandaw/HeartCodec-oss-20260123-burn";
const HEARTMULA_TOKENIZER_REL: &str = "tokenizer.json";
const HEARTMULA_GEN_CONFIG_REL: &str = "gen_config.json";

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
        "generate: mode=cli model={} backend={} sampler=topk-temperature",
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

fn cuda_feature_error() -> anyhow::Error {
    anyhow!("CUDA backend requested, but this build was compiled without the `cuda` feature")
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
    let model_paths = resolve_heartmula_model_paths(options.model_dir.as_deref())?;
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
        BackendChoice::Cuda => {
            #[cfg(feature = "cuda")]
            {
                return run_decode_only_with_backend::<burn::backend::Cuda<f32, i64>>(
                    options,
                    &model_paths.heartcodec_model_dir,
                    frames_json,
                );
            }
            #[cfg(not(feature = "cuda"))]
            {
                Err(cuda_feature_error())
            }
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
        .arg("heartmula")
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
    if let Some(tags) = &options.negative_prompt {
        command.arg("--tags").arg(tags);
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
        "generate: mode=ipc model={} backend={} sampler=topk-temperature",
        model_name(options.model),
        backend_name(options.backend),
    );
    let output = generate_heartmula_placeholder_output(&options)?;
    write_ipc_message(&mut stdout, &output.header)?;
    write_ipc_bytes(&mut stdout, &output.wav_bytes)?;
    eprintln!(
        "generate: mode=ipc complete backend={} wav_bytes={}",
        backend_name(options.backend),
        output.wav_bytes.len()
    );
    Ok(())
}

struct GeneratedOutput {
    header: GenerateResponseHeader,
    wav_bytes: Vec<u8>,
}

fn model_name(_model: ModelChoice) -> &'static str {
    "heartmula"
}

fn backend_name(backend: BackendChoice) -> &'static str {
    match backend {
        BackendChoice::Cpu => "cpu",
        BackendChoice::Vulkan => "vulkan",
        BackendChoice::Cuda => "cuda",
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

fn resolve_heartmula_model_paths(model_dir_override: Option<&Path>) -> Result<HeartmulaModelPaths> {
    let (model_dir, heartcodec_model_dir) = if let Some(model_dir) = model_dir_override {
        (model_dir.to_path_buf(), model_dir.to_path_buf())
    } else {
        let heartmula_snapshot_dir =
            ensure_repo_snapshot_dir(HEARTMULA_REPO_ID, &heartmula_required_relative_files())?;
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

fn generate_heartmula_placeholder_output(
    options: &maolan_generate::CliOptions,
) -> Result<GeneratedOutput> {
    let model_paths = resolve_heartmula_model_paths(options.model_dir.as_deref())?;
    anyhow::bail!(
        "HeartMula assets are available in {}, but the Burn runtime is not implemented yet. Found {}, {}, {}, and {}. The next step is to add generated/runtime model modules to generate and map these burnpacks into actual Burn modules.",
        model_paths.model_dir.display(),
        model_paths.heartmula_raw_bpk.display(),
        model_paths.heartcodec_raw_bpk.display(),
        model_paths.tokenizer_json.display(),
        model_paths.gen_config_json.display(),
    )
}

fn inspect_heartmula_cli(options: &maolan_generate::CliOptions) -> Result<()> {
    let model_paths = resolve_heartmula_model_paths(options.model_dir.as_deref())?;
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
        BackendChoice::Cuda => run_heartmula_cuda(options),
    }
}

fn run_heartmula_with_backend<B: Backend>(
    options: &maolan_generate::CliOptions,
    backend: BackendChoice,
) -> Result<()>
where
    B::Device: Default,
{
    let model_paths = resolve_heartmula_model_paths(options.model_dir.as_deref())?;
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
            .negative_prompt
            .as_deref()
            .unwrap_or(heartmula_runtime::default_tags()),
    );
    let lyrics = options.prompt.trim().to_lowercase();
    let lyrics_ids = heartmula_runtime::tokenize_text(&model_paths.tokenizer_json, &lyrics)?;
    let tags_ids = heartmula_runtime::tokenize_text(&model_paths.tokenizer_json, &tags)?;
    let max_audio_frames = ((options.length.max(1) as usize) / 80).max(1);
    let generation_config = heartmula_runtime::HeartmulaGenerationConfig {
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
    };
    let frames = model.generate_frames(&device, &generation_config)?;
    let generated_frame_count = frames.len();
    let frames_json_path = options.output_path.with_extension("frames.json");
    heartmula_runtime::write_frames_json(&frames_json_path, &lyrics, &tags, &frames)?;
    println!("frames_predecode={}", frames_json_path.display());
    if env::var_os(HEARTMULA_GENERATE_ONLY_ENV).is_some() {
        eprintln!("generate: unloading HeartMuLa before process exit");
        drop(model);
        println!("generate");
        println!("model=heartmula");
        println!("mode=tokens");
        println!("model_dir={}", model_paths.model_dir.display());
        println!("prompt={}", lyrics);
        if let Some(negative_prompt) = &options.negative_prompt {
            println!(
                "tags={}",
                heartmula_runtime::normalize_tags(negative_prompt)
            );
        }
        println!("backend={}", backend_name(backend));
        println!("cfg_scale={}", options.cfg_scale);
        println!("generated_frame_count={}", generated_frame_count);
        println!("frames_json={}", frames_json_path.display());
        println!("runtime=heartmula-burn");
        println!("note=HeartMuLa token generation completed in a dedicated subprocess");
        return Ok(());
    }
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
    println!("prompt={}", lyrics);
    if let Some(negative_prompt) = &options.negative_prompt {
        println!(
            "tags={}",
            heartmula_runtime::normalize_tags(negative_prompt)
        );
    }
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

fn run_heartmula_cuda(options: &maolan_generate::CliOptions) -> Result<()> {
    #[cfg(feature = "cuda")]
    {
        return run_heartmula_with_backend::<burn::backend::Cuda<f32, i64>>(
            options,
            BackendChoice::Cuda,
        );
    }
    #[cfg(not(feature = "cuda"))]
    {
        let _ = options;
        Err(cuda_feature_error())
    }
}
#[cfg(test)]
mod tests {
    use super::{
        HEARTMULA_GEN_CONFIG_REL, HEARTMULA_TOKENIZER_REL, HeartmulaModelPaths,
        count_numbered_layers, ensure_heartmula_model_paths, heartcodec_raw_bpk_rel,
        heartmula_raw_bpk_rel, infer_heartmula_runtime_summary,
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
}
