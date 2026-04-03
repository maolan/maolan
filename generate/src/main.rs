use anyhow::{Context, Result, anyhow};
use burn::prelude::Backend;
use burn::tensor::{Int, Tensor, TensorData};
use burn_store::{BurnpackStore, ModuleSnapshot, ModuleStore, TensorSnapshot};
use half::f16;
use maolan_generate::heartmula_runtime;
use maolan_generate::{
    BackendChoice, DEFAULT_MAX_PROMPT_TOKENS, FloatSize, GenerateResponseHeader, ModelChoice,
    SamplerChoice, encode_prompt, help_text, load_tokenizer, parse_options, read_ipc_message,
    validate_options, write_ipc_bytes, write_ipc_message,
};
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::collections::BTreeMap;
use std::env;
use std::f32::consts::PI;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

include!(concat!(env!("OUT_DIR"), "/model_bindings.rs"));

const LATENT_CHANNELS: usize = 64;
const LATENT_LENGTH: usize = 256;
const AUDIO_SAMPLE_RATE: usize = 44_100;
const SIGMA_MIN: f32 = 0.03;
const SIGMA_MAX: f32 = 1000.0;
const SIGMA_RHO: f32 = 1.0;
const DEFAULT_ETA: f32 = 1.0;
const DEFAULT_S_NOISE: f32 = 1.0;
const DEFAULT_SEED: u64 = 0;
const DEFAULT_SECONDS_START: i64 = 0;
const IPC_MODE_ENV: &str = "MAOLAN_BURN_SOCKETPAIR";
const HEARTMULA_LAZY_GENERATE_ONLY_ENV: &str = "MAOLAN_HEARTMULA_LAZY_GENERATE_ONLY";
const MODEL_DIR_ENV: &str = "MAOLAN_BURN_MODEL_DIR";
const HEARTMULA_MODEL_DIR_ENV: &str = "HEARTMULA_BURN_MODEL_DIR";
const DEFAULT_RUNTIME_MODEL_REPO_DIR: &str =
    ".cache/huggingface/hub/models--kurbloid--stable-audio-open-1.0-burn";
const DEFAULT_HEARTMULA_MODEL_REPO_DIR: &str =
    "repos/heartmula-burn/artifacts/heartmula-happy-new-year-20260123";
const T5_BPK_REL: &str = "burn_t5/stable_audio_t5_sim.bpk";
const DIT_BPK_REL: &str = "burn_dit/stable_audio_dit.bpk";
const VAE_BPK_REL: &str = "burn_vae/stable_audio_vae_decoder_sim.bpk";
const HEARTMULA_TOKENIZER_REL: &str = "tokenizer.json";
const HEARTMULA_GEN_CONFIG_REL: &str = "gen_config.json";

struct RuntimeModelPaths {
    model_dir: PathBuf,
    t5_bpk: String,
    dit_bpk: String,
    vae_bpk: String,
}

struct HeartmulaModelPaths {
    model_dir: PathBuf,
    float_size: FloatSize,
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

#[derive(Clone, Copy)]
struct SamplingConfig {
    sampler: SamplerChoice,
    guidance_scale: f32,
}

struct BrownianNoiseSampler {
    interval_noises: Vec<Vec<f32>>,
    channels: usize,
    latent_length: usize,
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

    if options.model == ModelChoice::Heartmula
        && options.lazy
        && env::var_os(HEARTMULA_LAZY_GENERATE_ONLY_ENV).is_none()
    {
        return run_heartmula_lazy_supervisor(&options);
    }

    match options.model {
        ModelChoice::StableAudioOpen => eprintln!(
            "generate: mode=cli model={} backend={} sampler={}",
            model_name(options.model),
            backend_name(options.backend),
            sampler_name(options.sampler),
        ),
        ModelChoice::Heartmula => eprintln!(
            "generate: mode=cli model={} backend={} sampler=topk-temperature",
            model_name(options.model),
            backend_name(options.backend),
        ),
    }

    match options.model {
        ModelChoice::StableAudioOpen => match options.backend {
            BackendChoice::Cpu => run_cpu(&options),
            BackendChoice::Vulkan => run_vulkan(&options),
            BackendChoice::Cuda => run_cuda(&options),
        },
        ModelChoice::Heartmula => run_heartmula_cli(&options),
    }
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
    let model_dir = resolve_heartmula_model_dir(options.model_dir.as_deref())?;
    println!("decode_only_model_dir={}", model_dir.display());
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
            &model_dir,
            frames_json,
        ),
        BackendChoice::Vulkan => {
            let device = burn::backend::wgpu::WgpuDevice::default();
            burn::backend::wgpu::init_setup::<burn::backend::wgpu::graphics::Vulkan>(
                &device,
                vulkan_runtime_options(),
            );
            match options.float_size {
                FloatSize::F16 => {
                    run_decode_only_with_backend::<burn::backend::Wgpu<f16, i64, u32>>(
                        options,
                        &model_dir,
                        frames_json,
                    )
                }
                FloatSize::F32 => {
                    run_decode_only_with_backend::<burn::backend::Wgpu<f32, i64, u32>>(
                        options,
                        &model_dir,
                        frames_json,
                    )
                }
            }
        }
        BackendChoice::Cuda => {
            #[cfg(feature = "cuda")]
            {
                return run_decode_only_with_backend::<burn::backend::Cuda<f32, i64>>(
                    options,
                    &model_dir,
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
        float_size_name(options.float_size),
        frames_json,
        &options.output_path,
        options.seconds_total as f32,
        &Default::default(),
        options.ode_steps,
        options.decoder,
        options.decoder_seed,
    )
}

fn spawn_lazy_generate_subprocess(options: &maolan_generate::CliOptions) -> Result<()> {
    let current_exe =
        env::current_exe().context("failed to resolve current executable for lazy generate")?;
    let mut command = Command::new(current_exe);
    command
        .env(HEARTMULA_LAZY_GENERATE_ONLY_ENV, "1")
        .arg("--model")
        .arg("heartmula")
        .arg("--backend")
        .arg(backend_name(options.backend))
        .arg("--float-size")
        .arg(float_size_name(options.float_size))
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
        .arg("--seconds-total")
        .arg(options.seconds_total.to_string())
        .arg("--ode-steps")
        .arg(options.ode_steps.to_string());
    if let Some(model_dir) = &options.model_dir {
        command.arg("--model-dir").arg(model_dir);
    }
    if let Some(tags) = &options.negative_prompt {
        command.arg("--tags").arg(tags);
    }
    if let Some(max_audio_length_ms) = options.max_audio_length_ms {
        command
            .arg("--max-audio-length-ms")
            .arg(max_audio_length_ms.to_string());
    }
    let status = command
        .status()
        .context("failed to spawn lazy generation subprocess")?;
    if !status.success() {
        anyhow::bail!("lazy generation subprocess failed with status {status}");
    }
    Ok(())
}

fn run_heartmula_lazy_supervisor(options: &maolan_generate::CliOptions) -> Result<()> {
    let frames_json_path = options.output_path.with_extension("frames.json");
    eprintln!("generate: lazy=true spawning HeartMuLa generation subprocess");
    spawn_lazy_generate_subprocess(options)?;
    eprintln!("generate: lazy=true generation subprocess exited; starting in-process decode");
    run_decode_with_frames_json(options, &frames_json_path)
}

fn run_ipc() -> Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let options = validate_options(read_ipc_message(&mut stdin)?)?;
    match options.model {
        ModelChoice::StableAudioOpen => eprintln!(
            "generate: mode=ipc model={} backend={} sampler={}",
            model_name(options.model),
            backend_name(options.backend),
            sampler_name(options.sampler),
        ),
        ModelChoice::Heartmula => eprintln!(
            "generate: mode=ipc model={} backend={} sampler=topk-temperature",
            model_name(options.model),
            backend_name(options.backend),
        ),
    }
    let output = match options.model {
        ModelChoice::StableAudioOpen => match options.backend {
            BackendChoice::Cpu => generate_cpu_output(&options),
            BackendChoice::Vulkan => generate_vulkan_output(&options),
            BackendChoice::Cuda => generate_cuda_output(&options),
        },
        ModelChoice::Heartmula => generate_heartmula_placeholder_output(&options),
    }?;
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

fn load_text_model<B: Backend>(
    model_paths: &RuntimeModelPaths,
    device: &B::Device,
) -> Result<stable_audio_t5::Model<B>> {
    let mut model = stable_audio_t5::Model::<B>::new(device);
    let mut store = BurnpackStore::from_file(&model_paths.t5_bpk).zero_copy(true);
    model.load_from(&mut store).with_context(|| {
        format!(
            "failed to load T5 weights from {}. The runtime .bpk files may not match the compiled generated sources from {}",
            model_paths.t5_bpk, GENERATED_SOURCE_DIR
        )
    })?;
    Ok(model)
}

fn load_dit_model<B: Backend>(
    model_paths: &RuntimeModelPaths,
    device: &B::Device,
) -> Result<stable_audio_dit::Model<B>> {
    let mut model = stable_audio_dit::Model::<B>::new(device);
    let mut store = BurnpackStore::from_file(&model_paths.dit_bpk).zero_copy(true);
    model.load_from(&mut store).with_context(|| {
        format!(
            "failed to load DiT weights from {}. The runtime .bpk files may not match the compiled generated sources from {}",
            model_paths.dit_bpk, GENERATED_SOURCE_DIR
        )
    })?;
    Ok(model)
}

fn run_with_backend<B>(
    options: &maolan_generate::CliOptions,
    backend: BackendChoice,
) -> Result<GeneratedOutput>
where
    B: Backend,
    B::Device: Default,
{
    let target_frames = target_audio_frames(options.seconds_total);
    let model_paths = resolve_runtime_model_paths(options.model_dir.as_deref())?;
    let tokenizer = load_tokenizer()?;
    let device = Default::default();
    let seconds_start_tensor = Tensor::<B, 1, Int>::from_data(
        TensorData::new(vec![DEFAULT_SECONDS_START], [1]).convert::<B::IntElem>(),
        &device,
    );
    let seconds_total_tensor = Tensor::<B, 1, Int>::from_data(
        TensorData::new(vec![options.seconds_total], [1]).convert::<B::IntElem>(),
        &device,
    );
    let text_model = load_text_model::<B>(&model_paths, &device)?;
    let (text_embeddings, attention_mask_len) = encode_text_embeddings(
        &tokenizer,
        &text_model,
        &options.prompt,
        seconds_start_tensor.clone(),
        seconds_total_tensor.clone(),
        &device,
    )?;
    let negative_text_embeddings = if let Some(negative_prompt) = &options.negative_prompt {
        encode_text_embeddings(
            &tokenizer,
            &text_model,
            negative_prompt,
            seconds_start_tensor,
            seconds_total_tensor,
            &device,
        )?
        .0
    } else {
        Tensor::<B, 3>::zeros(text_embeddings.dims(), &device)
    };
    let dit_model = load_dit_model::<B>(&model_paths, &device)?;

    let sampling_config = SamplingConfig {
        sampler: options.sampler,
        guidance_scale: options.cfg_scale,
    };
    let sigmas = polyexponential_sigmas(options.steps, SIGMA_MIN, SIGMA_MAX, SIGMA_RHO);
    let mut rng = SmallRng::seed_from_u64(DEFAULT_SEED);
    let latents = sample_latents::<B>(
        &dit_model,
        &sigmas,
        sampling_config,
        text_embeddings.clone(),
        negative_text_embeddings.clone(),
        &device,
        &mut rng,
    );

    let decoded = decode_vae_chunked::<B>(latents, &device, &model_paths.vae_bpk)?;
    let dims = decoded.dims();
    let audio = trim_audio_frames(decoded.to_data().to_vec::<f32>()?, dims[1], target_frames);
    let normalized_audio = normalize_audio_peak(&audio);
    let frames = target_frames.min(dims[2]);
    let wav_bytes = write_wav_bytes(&normalized_audio, dims[1], frames)?;
    Ok(GeneratedOutput {
        header: GenerateResponseHeader {
            backend,
            channels: dims[1],
            frames,
            guidance_scale: options.cfg_scale,
            prompt_tokens: attention_mask_len,
            sample_rate_hz: AUDIO_SAMPLE_RATE as u32,
            sampler: options.sampler,
            seconds_total: options.seconds_total,
            steps: options.steps,
            wav_bytes_len: wav_bytes.len(),
        },
        wav_bytes,
    })
}

fn encode_text_embeddings<B: Backend>(
    tokenizer: &sentencepiece::SentencePieceProcessor,
    text_model: &stable_audio_t5::Model<B>,
    prompt: &str,
    seconds_start_tensor: Tensor<B, 1, Int>,
    seconds_total_tensor: Tensor<B, 1, Int>,
    device: &B::Device,
) -> Result<(Tensor<B, 3>, i64)> {
    let (token_ids, attention_mask) = encode_prompt(tokenizer, prompt, DEFAULT_MAX_PROMPT_TOKENS)?;
    let attention_mask_len = attention_mask.iter().copied().sum::<i64>();
    let token_tensor = Tensor::<B, 2, Int>::from_data(
        TensorData::new(token_ids, [1, DEFAULT_MAX_PROMPT_TOKENS]).convert::<B::IntElem>(),
        device,
    );
    let attention_mask_tensor = Tensor::<B, 2, Int>::from_data(
        TensorData::new(attention_mask, [1, DEFAULT_MAX_PROMPT_TOKENS]).convert::<B::IntElem>(),
        device,
    );
    let embeddings = text_model.forward(
        token_tensor,
        attention_mask_tensor,
        seconds_start_tensor,
        seconds_total_tensor,
    );
    Ok((embeddings, attention_mask_len))
}

fn polyexponential_sigmas(
    num_inference_steps: usize,
    sigma_min: f32,
    sigma_max: f32,
    rho: f32,
) -> Vec<f32> {
    if num_inference_steps == 0 {
        return vec![0.0];
    }

    let log_min = sigma_min.ln();
    let log_span = sigma_max.ln() - log_min;
    let mut sigmas = (0..num_inference_steps)
        .map(|index| {
            let ramp = if num_inference_steps == 1 {
                1.0
            } else {
                1.0 - index as f32 / (num_inference_steps - 1) as f32
            };
            ((ramp.powf(rho) * log_span) + log_min).exp()
        })
        .collect::<Vec<_>>();
    sigmas.push(0.0);
    sigmas
}

fn sigma_to_t(sigma: f32) -> f32 {
    sigma.atan() * (2.0 / PI)
}

fn sampler_name(sampler: SamplerChoice) -> &'static str {
    match sampler {
        SamplerChoice::Dpmpp2m => "dpmpp-2m",
        SamplerChoice::Dpmpp3mSde => "dpmpp-3m-sde",
    }
}

fn model_name(model: ModelChoice) -> &'static str {
    match model {
        ModelChoice::StableAudioOpen => "stable-audio-open",
        ModelChoice::Heartmula => "heartmula",
    }
}

fn backend_name(backend: BackendChoice) -> &'static str {
    match backend {
        BackendChoice::Cpu => "cpu",
        BackendChoice::Vulkan => "vulkan",
        BackendChoice::Cuda => "cuda",
    }
}

fn float_size_name(float_size: FloatSize) -> &'static str {
    match float_size {
        FloatSize::F16 => "f16",
        FloatSize::F32 => "f32",
    }
}

fn write_cli_output(
    options: &maolan_generate::CliOptions,
    backend_label: &str,
    output: &GeneratedOutput,
) -> Result<()> {
    let model_paths = resolve_runtime_model_paths(options.model_dir.as_deref())?;
    write_wav(&options.output_path, &output.wav_bytes)?;

    println!("generate");
    println!("model_dir={}", model_paths.model_dir.display());
    println!("prompt={}", options.prompt);
    if let Some(negative_prompt) = &options.negative_prompt {
        println!("negative_prompt={negative_prompt}");
    }
    println!("prompt_tokens={}", output.header.prompt_tokens);
    println!("backend={backend_label}");
    println!("inference_steps={}", options.steps);
    println!("guidance_scale={}", options.cfg_scale);
    println!("sampler={}", sampler_name(options.sampler));
    println!("seconds_total={}", options.seconds_total);
    println!(
        "target_frames={}",
        target_audio_frames(options.seconds_total)
    );
    println!("latent_length={LATENT_LENGTH}");
    println!("sigma_min={SIGMA_MIN}");
    println!("sigma_max={SIGMA_MAX}");
    println!("seed={DEFAULT_SEED}");
    println!("output={}", options.output_path.display());
    eprintln!(
        "generate: mode=cli complete backend={} output={} wav_bytes={}",
        backend_label,
        options.output_path.display(),
        output.wav_bytes.len()
    );

    Ok(())
}

fn target_audio_frames(seconds_total: i64) -> usize {
    let seconds = usize::try_from(seconds_total.max(0)).unwrap_or(usize::MAX / AUDIO_SAMPLE_RATE);
    seconds.saturating_mul(AUDIO_SAMPLE_RATE)
}

fn sample_latents<B: Backend>(
    dit_model: &stable_audio_dit::Model<B>,
    sigmas: &[f32],
    sampling_config: SamplingConfig,
    text_embeddings: Tensor<B, 3>,
    null_text_embeddings: Tensor<B, 3>,
    device: &B::Device,
    rng: &mut SmallRng,
) -> Tensor<B, 3> {
    match sampling_config.sampler {
        SamplerChoice::Dpmpp2m => sample_dpmpp_2m(
            dit_model,
            sigmas,
            sampling_config.guidance_scale,
            text_embeddings,
            null_text_embeddings,
            device,
            rng,
        ),
        SamplerChoice::Dpmpp3mSde => sample_dpmpp_3m_sde(
            dit_model,
            sigmas,
            sampling_config.guidance_scale,
            text_embeddings,
            null_text_embeddings,
            device,
            rng,
        ),
    }
}

fn sample_dpmpp_2m<B: Backend>(
    dit_model: &stable_audio_dit::Model<B>,
    sigmas: &[f32],
    guidance_scale: f32,
    text_embeddings: Tensor<B, 3>,
    null_text_embeddings: Tensor<B, 3>,
    device: &B::Device,
    rng: &mut SmallRng,
) -> Tensor<B, 3> {
    let mut latents = initial_latents::<B>(device, rng, LATENT_LENGTH).mul_scalar(sigmas[0]);
    let mut old_denoised: Option<Tensor<B, 3>> = None;

    for (index, sigma_pair) in sigmas.windows(2).enumerate() {
        let sigma = sigma_pair[0];
        let sigma_next = sigma_pair[1];
        let denoised = predict_denoised::<B>(
            dit_model,
            latents.clone(),
            sigma,
            guidance_scale,
            text_embeddings.clone(),
            null_text_embeddings.clone(),
            device,
        );

        if sigma_next == 0.0 {
            latents = denoised;
            break;
        }

        let ratio = sigma_next / sigma;
        let h = sigma.ln() - sigma_next.ln();
        let denoised_delta = if let Some(previous) = old_denoised.clone() {
            if index > 0 {
                let h_last = sigmas[index - 1].ln() - sigma.ln();
                let r = h_last / h;
                denoised
                    .clone()
                    .mul_scalar(1.0 + 1.0 / (2.0 * r))
                    .sub(previous.mul_scalar(1.0 / (2.0 * r)))
            } else {
                denoised.clone()
            }
        } else {
            denoised.clone()
        };

        latents = latents
            .mul_scalar(ratio)
            .add(denoised_delta.mul_scalar(1.0 - ratio));
        old_denoised = Some(denoised);
    }

    latents
}

fn sample_dpmpp_3m_sde<B: Backend>(
    dit_model: &stable_audio_dit::Model<B>,
    sigmas: &[f32],
    guidance_scale: f32,
    text_embeddings: Tensor<B, 3>,
    null_text_embeddings: Tensor<B, 3>,
    device: &B::Device,
    rng: &mut SmallRng,
) -> Tensor<B, 3> {
    let mut latents = initial_latents::<B>(device, rng, LATENT_LENGTH).mul_scalar(sigmas[0]);
    let noise_sampler = BrownianNoiseSampler::new(sigmas, rng, LATENT_CHANNELS, LATENT_LENGTH);
    let mut denoised_1: Option<Tensor<B, 3>> = None;
    let mut denoised_2: Option<Tensor<B, 3>> = None;
    let mut h_1: Option<f32> = None;
    let mut h_2: Option<f32> = None;

    for (index, sigma_pair) in sigmas.windows(2).enumerate() {
        let sigma = sigma_pair[0];
        let sigma_next = sigma_pair[1];
        let denoised = predict_denoised::<B>(
            dit_model,
            latents.clone(),
            sigma,
            guidance_scale,
            text_embeddings.clone(),
            null_text_embeddings.clone(),
            device,
        );

        if sigma_next == 0.0 {
            latents = denoised.clone();
        } else {
            let t = -sigma.ln();
            let s = -sigma_next.ln();
            let h = s - t;
            let h_eta = h * (DEFAULT_ETA + 1.0);

            latents = latents
                .mul_scalar((-h_eta).exp())
                .add(denoised.clone().mul_scalar(-(-h_eta).exp_m1()));

            if let (Some(prev_1), Some(prev_2), Some(prev_h1), Some(prev_h2)) =
                (denoised_1.clone(), denoised_2.clone(), h_1, h_2)
            {
                let r0 = prev_h1 / h;
                let r1 = prev_h2 / h;
                let d1_0 = denoised.clone().sub(prev_1.clone()).mul_scalar(1.0 / r0);
                let d1_1 = prev_1.sub(prev_2).mul_scalar(1.0 / r1);
                let d1 = d1_0
                    .clone()
                    .add(d1_0.clone().sub(d1_1.clone()).mul_scalar(r0 / (r0 + r1)));
                let d2 = d1_0.sub(d1_1).mul_scalar(1.0 / (r0 + r1));
                let phi_2 = (-h_eta).exp_m1() / h_eta + 1.0;
                let phi_3 = phi_2 / h_eta - 0.5;
                latents = latents.add(d1.mul_scalar(phi_2)).sub(d2.mul_scalar(phi_3));
            } else if let (Some(prev_1), Some(prev_h1)) = (denoised_1.clone(), h_1) {
                let r = prev_h1 / h;
                let d = denoised.clone().sub(prev_1).mul_scalar(1.0 / r);
                let phi_2 = (-h_eta).exp_m1() / h_eta + 1.0;
                latents = latents.add(d.mul_scalar(phi_2));
            }

            if DEFAULT_ETA != 0.0 {
                let noise_scale =
                    sigma_next * (-(-2.0 * h * DEFAULT_ETA).exp_m1()).sqrt() * DEFAULT_S_NOISE;
                latents = latents.add(
                    noise_sampler
                        .sample::<B>(index, device)
                        .mul_scalar(noise_scale),
                );
            }

            h_2 = h_1;
            h_1 = Some(h);
        }

        denoised_2 = denoised_1;
        denoised_1 = Some(denoised);
    }

    latents
}

fn predict_denoised<B: Backend>(
    dit_model: &stable_audio_dit::Model<B>,
    latents: Tensor<B, 3>,
    sigma: f32,
    guidance_scale: f32,
    text_embeddings: Tensor<B, 3>,
    null_text_embeddings: Tensor<B, 3>,
    device: &B::Device,
) -> Tensor<B, 3> {
    let sigma_sq = sigma * sigma;
    let denom_sqrt = (sigma_sq + 1.0).sqrt();
    let c_skip = 1.0 / (sigma_sq + 1.0);
    let c_out = -sigma / denom_sqrt;
    let c_in = 1.0 / denom_sqrt;
    let timestep = Tensor::<B, 1>::from_data([sigma_to_t(sigma)], device);
    let scaled_latents = latents.clone().mul_scalar(c_in);
    let cond_v = dit_model.forward(scaled_latents.clone(), timestep.clone(), text_embeddings);
    let uncond_v = dit_model.forward(scaled_latents, timestep, null_text_embeddings);
    let guided_v = uncond_v.clone().add(
        cond_v
            .clone()
            .sub(uncond_v.clone())
            .mul_scalar(guidance_scale),
    );

    guided_v.mul_scalar(c_out).add(latents.mul_scalar(c_skip))
}

fn initial_latents<B: Backend>(
    device: &B::Device,
    rng: &mut SmallRng,
    latent_length: usize,
) -> Tensor<B, 3> {
    let values = (0..(LATENT_CHANNELS * latent_length))
        .map(|_| normal_sample(rng))
        .collect::<Vec<_>>();
    Tensor::<B, 3>::from_data(
        TensorData::new(values, [1, LATENT_CHANNELS, latent_length]),
        device,
    )
}

impl BrownianNoiseSampler {
    fn new(sigmas: &[f32], rng: &mut SmallRng, channels: usize, latent_length: usize) -> Self {
        let positive_sigmas = sigmas
            .iter()
            .copied()
            .filter(|sigma| *sigma > 0.0)
            .collect::<Vec<_>>();
        let element_count = channels.saturating_mul(latent_length);

        if positive_sigmas.len() < 2 || element_count == 0 {
            return Self {
                interval_noises: Vec::new(),
                channels,
                latent_length,
            };
        }

        let ascending_sigmas = positive_sigmas.iter().copied().rev().collect::<Vec<_>>();
        let mut ascending_states = Vec::with_capacity(ascending_sigmas.len());
        ascending_states.push(vec![0.0; element_count]);

        for sigma_pair in ascending_sigmas.windows(2) {
            let dt_sqrt = (sigma_pair[1] - sigma_pair[0]).max(0.0).sqrt();
            let previous_state = ascending_states
                .last()
                .expect("brownian path must start with an initial state");
            let next_state = previous_state
                .iter()
                .map(|value| value + normal_sample(rng) * dt_sqrt)
                .collect::<Vec<_>>();
            ascending_states.push(next_state);
        }

        let descending_states = ascending_states.into_iter().rev().collect::<Vec<_>>();
        let interval_noises = descending_states
            .windows(2)
            .zip(positive_sigmas.windows(2))
            .map(|(state_pair, sigma_pair)| {
                let dt_sqrt = (sigma_pair[0] - sigma_pair[1]).max(f32::EPSILON).sqrt();
                let current_state = &state_pair[0];
                let next_state = &state_pair[1];
                next_state
                    .iter()
                    .zip(current_state.iter())
                    .map(|(next, current)| (next - current) / dt_sqrt)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        Self {
            interval_noises,
            channels,
            latent_length,
        }
    }

    fn sample<B: Backend>(&self, interval_index: usize, device: &B::Device) -> Tensor<B, 3> {
        let values = self
            .interval_noises
            .get(interval_index)
            .cloned()
            .unwrap_or_else(|| vec![0.0; self.channels.saturating_mul(self.latent_length)]);
        Tensor::<B, 3>::from_data(
            TensorData::new(values, [1, self.channels, self.latent_length]),
            device,
        )
    }
}

fn normal_sample(rng: &mut SmallRng) -> f32 {
    let u1 = rng.random_range(f32::EPSILON..1.0);
    let u2 = rng.random_range(0.0f32..1.0f32);
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

fn write_wav(path: &Path, wav_bytes: &[u8]) -> Result<()> {
    std::fs::write(path, wav_bytes)
        .with_context(|| format!("failed to write '{}'", path.display()))?;
    Ok(())
}

fn write_wav_bytes(samples: &[f32], channels: usize, frames: usize) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: channels as u16,
        sample_rate: AUDIO_SAMPLE_RATE as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut cursor, spec)?;

    for frame in 0..frames {
        for channel in 0..channels {
            let index = channel * frames + frame;
            let sample = samples
                .get(index)
                .copied()
                .unwrap_or_default()
                .clamp(-1.0, 1.0);
            writer.write_sample(sample)?;
        }
    }

    writer.finalize()?;
    Ok(cursor.into_inner())
}

fn normalize_audio_peak(samples: &[f32]) -> Vec<f32> {
    let peak = samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0f32, f32::max);

    if peak <= f32::EPSILON {
        return samples.to_vec();
    }

    samples.iter().map(|sample| sample / peak).collect()
}

fn trim_audio_frames(samples: Vec<f32>, channels: usize, target_frames: usize) -> Vec<f32> {
    let desired_len = channels.saturating_mul(target_frames);
    samples.into_iter().take(desired_len).collect()
}

fn decode_vae_chunked<B: Backend>(
    latent: Tensor<B, 3>,
    device: &B::Device,
    vae_bpk: &str,
) -> Result<Tensor<B, 3>> {
    let mut model = stable_audio_vae::Model::<B>::new(device);
    let mut store = BurnpackStore::from_file(vae_bpk).zero_copy(true);
    model.load_from(&mut store).with_context(|| {
        format!(
            "failed to load VAE weights from {}. The runtime .bpk files may not match the compiled generated sources from {}",
            vae_bpk, GENERATED_SOURCE_DIR
        )
    })?;
    Ok(model.forward(latent))
}

fn resolve_runtime_model_paths(model_dir_override: Option<&Path>) -> Result<RuntimeModelPaths> {
    let model_dir = resolve_model_dir(
        model_dir_override
            .map(Path::as_os_str)
            .or(env::var_os(MODEL_DIR_ENV).as_deref()),
    )?;
    Ok(RuntimeModelPaths {
        t5_bpk: model_dir.join(T5_BPK_REL).display().to_string(),
        dit_bpk: model_dir.join(DIT_BPK_REL).display().to_string(),
        vae_bpk: model_dir.join(VAE_BPK_REL).display().to_string(),
        model_dir,
    })
}

fn resolve_heartmula_model_dir(model_dir_override: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = model_dir_override.map(Path::to_path_buf) {
        return Ok(path);
    }

    if let Some(path) = env::var_os(HEARTMULA_MODEL_DIR_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(path);
    }

    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    Ok(PathBuf::from(home).join(DEFAULT_HEARTMULA_MODEL_REPO_DIR))
}

fn heartmula_raw_bpk_rel(float_size: FloatSize) -> &'static str {
    match float_size {
        FloatSize::F16 => "burn_raw/heartmula_raw_f16.bpk",
        FloatSize::F32 => "burn_raw/heartmula_raw_f32.bpk",
    }
}

fn heartcodec_raw_bpk_rel(float_size: FloatSize) -> &'static str {
    match float_size {
        FloatSize::F16 => "burn_raw/heartcodec_raw_f16.bpk",
        FloatSize::F32 => "burn_raw/heartcodec_raw_f32.bpk",
    }
}

fn resolve_heartmula_model_paths(
    model_dir_override: Option<&Path>,
    float_size: FloatSize,
) -> Result<HeartmulaModelPaths> {
    let model_dir = resolve_heartmula_model_dir(model_dir_override)?;
    let paths = HeartmulaModelPaths {
        float_size,
        heartmula_raw_bpk: model_dir.join(heartmula_raw_bpk_rel(float_size)),
        heartcodec_raw_bpk: model_dir.join(heartcodec_raw_bpk_rel(float_size)),
        tokenizer_json: model_dir.join(HEARTMULA_TOKENIZER_REL),
        gen_config_json: model_dir.join(HEARTMULA_GEN_CONFIG_REL),
        model_dir,
    };
    ensure_heartmula_model_paths(&paths)?;
    Ok(paths)
}

fn ensure_heartmula_model_paths(paths: &HeartmulaModelPaths) -> Result<()> {
    let missing = [
        (
            heartmula_raw_bpk_rel(paths.float_size),
            &paths.heartmula_raw_bpk,
        ),
        (
            heartcodec_raw_bpk_rel(paths.float_size),
            &paths.heartcodec_raw_bpk,
        ),
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
        "HeartMula assets are incomplete in {}. Missing: {}",
        paths.model_dir.display(),
        missing.join(", ")
    )
}

fn resolve_model_dir(env_value: Option<&std::ffi::OsStr>) -> Result<PathBuf> {
    if let Some(path) = env_value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(path);
    }

    resolve_hugging_face_snapshot_dir(&default_hf_repo_cache_dir()).ok_or_else(|| {
        anyhow!(
            "failed to locate generate model snapshot in Hugging Face hub cache; set {MODEL_DIR_ENV} or download the model via maolan"
        )
    })
}

fn default_hf_repo_cache_dir() -> PathBuf {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(DEFAULT_RUNTIME_MODEL_REPO_DIR)
}

fn resolve_hugging_face_snapshot_dir(repo_cache_dir: &Path) -> Option<PathBuf> {
    let refs_main = repo_cache_dir.join("refs").join("main");
    if let Ok(revision) = std::fs::read_to_string(&refs_main) {
        let snapshot_dir = repo_cache_dir.join("snapshots").join(revision.trim());
        if has_required_model_files(&snapshot_dir) {
            return Some(snapshot_dir);
        }
    }

    let snapshots_dir = repo_cache_dir.join("snapshots");
    let mut snapshots = std::fs::read_dir(snapshots_dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| has_required_model_files(path))
        .collect::<Vec<_>>();
    snapshots.sort();
    snapshots.pop()
}

fn has_required_model_files(snapshot_dir: &Path) -> bool {
    snapshot_dir.join(T5_BPK_REL).exists()
        && snapshot_dir.join(DIT_BPK_REL).exists()
        && snapshot_dir.join(VAE_BPK_REL).exists()
}

fn generate_cpu_output(options: &maolan_generate::CliOptions) -> Result<GeneratedOutput> {
    run_with_backend::<burn::backend::NdArray<f32>>(options, BackendChoice::Cpu)
}

fn run_cpu(options: &maolan_generate::CliOptions) -> Result<()> {
    let output = generate_cpu_output(options)?;
    write_cli_output(options, "cpu", &output)
}

fn generate_vulkan_output(options: &maolan_generate::CliOptions) -> Result<GeneratedOutput> {
    let device = burn::backend::wgpu::WgpuDevice::default();
    burn::backend::wgpu::init_setup::<burn::backend::wgpu::graphics::Vulkan>(
        &device,
        vulkan_runtime_options(),
    );
    run_with_backend::<burn::backend::Wgpu<f32, i64, u32>>(options, BackendChoice::Vulkan)
}

fn run_vulkan(options: &maolan_generate::CliOptions) -> Result<()> {
    let output = generate_vulkan_output(options)?;
    write_cli_output(options, "vulkan", &output)
}

#[cfg(feature = "cuda")]
fn generate_cuda_output(options: &maolan_generate::CliOptions) -> Result<GeneratedOutput> {
    run_with_backend::<burn::backend::Cuda<f32, i64>>(options, BackendChoice::Cuda)
}

#[cfg(not(feature = "cuda"))]
fn generate_cuda_output(_options: &maolan_generate::CliOptions) -> Result<GeneratedOutput> {
    Err(cuda_feature_error())
}

fn generate_heartmula_placeholder_output(
    options: &maolan_generate::CliOptions,
) -> Result<GeneratedOutput> {
    let model_paths =
        resolve_heartmula_model_paths(options.model_dir.as_deref(), options.float_size)?;
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
    let model_paths =
        resolve_heartmula_model_paths(options.model_dir.as_deref(), options.float_size)?;
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
    println!("float_size={}", float_size_name(model_paths.float_size));
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
    let model_paths =
        resolve_heartmula_model_paths(options.model_dir.as_deref(), options.float_size)?;
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
    let max_audio_frames = if let Some(ms) = options.max_audio_length_ms {
        ((ms.max(1) as usize) / 80).max(1)
    } else {
        ((options.seconds_total.max(1) as usize) * 1000) / 80
    };
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
    if env::var_os(HEARTMULA_LAZY_GENERATE_ONLY_ENV).is_some() {
        eprintln!("generate: lazy=true unloading HeartMuLa before process exit");
        drop(model);
        println!("generate");
        println!("model=heartmula");
        println!("mode=tokens");
        println!("float_size={}", float_size_name(model_paths.float_size));
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
        println!("lazy={}", options.lazy);
        println!("generated_frame_count={}", generated_frame_count);
        println!("frames_json={}", frames_json_path.display());
        println!("lazy_process_boundary=true");
        println!("runtime=heartmula-burn");
        println!("note=HeartMuLa token generation completed in a dedicated subprocess");
        return Ok(());
    }
    heartmula_runtime::decode_frames_to_wav::<B>(
        &model_paths.model_dir,
        backend_name(backend),
        float_size_name(model_paths.float_size),
        &frames_json_path,
        &options.output_path,
        options.seconds_total as f32,
        &device,
        options.ode_steps,
        options.decoder,
        options.decoder_seed,
    )?;
    println!("generate");
    println!("model=heartmula");
    println!("mode=tokens");
    println!("float_size={}", float_size_name(model_paths.float_size));
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
    println!("lazy={}", options.lazy);
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
    println!("lazy_process_boundary={}", options.lazy);
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
    match options.float_size {
        FloatSize::F16 => run_heartmula_with_backend::<burn::backend::Wgpu<f16, i64, u32>>(
            options,
            BackendChoice::Vulkan,
        ),
        FloatSize::F32 => run_heartmula_with_backend::<burn::backend::Wgpu<f32, i64, u32>>(
            options,
            BackendChoice::Vulkan,
        ),
    }
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
fn run_cuda(options: &maolan_generate::CliOptions) -> Result<()> {
    let output = generate_cuda_output(options)?;
    write_cli_output(options, "cuda", &output)
}

#[cfg(test)]
mod tests {
    use super::{
        BrownianNoiseSampler, HEARTMULA_GEN_CONFIG_REL, HEARTMULA_TOKENIZER_REL,
        HeartmulaModelPaths, count_numbered_layers, default_hf_repo_cache_dir,
        ensure_heartmula_model_paths, heartcodec_raw_bpk_rel, heartmula_raw_bpk_rel,
        infer_heartmula_runtime_summary, normalize_audio_peak, polyexponential_sigmas,
        resolve_heartmula_model_dir, resolve_model_dir, sampler_name, sigma_to_t,
        target_audio_frames, trim_audio_frames,
    };
    use crate::heartmula_runtime::normalize_tags;
    use maolan_generate::{FloatSize, SamplerChoice};
    use rand::{SeedableRng, rngs::SmallRng};
    use std::{collections::BTreeMap, env, ffi::OsStr, fs, path::PathBuf};

    #[test]
    fn polyexponential_sigmas_descend_and_end_with_zero() {
        let sigmas = polyexponential_sigmas(4, 0.03, 1000.0, 1.0);
        assert_eq!(sigmas.len(), 5);
        assert_eq!(sigmas.last().copied(), Some(0.0));
        assert!(sigmas[0] > sigmas[1]);
        assert!(sigmas[1] > sigmas[2]);
        assert!(sigmas[2] > sigmas[3]);
    }

    #[test]
    fn sigma_to_t_maps_zero_to_zero() {
        assert_eq!(sigma_to_t(0.0), 0.0);
        assert!(sigma_to_t(1.0) > 0.0);
    }

    #[test]
    fn normalize_tags_wraps_and_lowercases() {
        assert_eq!(normalize_tags("Piano,HAPPY"), "<tag>piano,happy</tag>");
    }

    #[test]
    fn normalize_tags_preserves_existing_wrappers() {
        assert_eq!(normalize_tags("<tag>piano</tag>"), "<tag>piano</tag>");
    }

    #[test]
    fn normalize_audio_peak_scales_to_unity() {
        let normalized = normalize_audio_peak(&[0.25, -0.5, 1.0, -2.0]);
        let peak = normalized
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0, f32::max);
        assert!((peak - 1.0).abs() < 1e-6);
    }

    #[test]
    fn default_sampler_matches_reference_default() {
        assert_eq!(sampler_name(SamplerChoice::Dpmpp3mSde), "dpmpp-3m-sde");
    }

    #[test]
    fn target_audio_frames_uses_model_sample_rate() {
        assert_eq!(target_audio_frames(10), 441_000);
    }

    #[test]
    fn trim_audio_frames_keeps_requested_channel_major_prefix() {
        let trimmed = trim_audio_frames(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 2);
        assert_eq!(trimmed, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn brownian_noise_sampler_creates_one_interval_per_positive_sigma_step() {
        let mut rng = SmallRng::seed_from_u64(0);
        let sampler = BrownianNoiseSampler::new(&[4.0, 2.0, 1.0, 0.0], &mut rng, 2, 3);
        assert_eq!(sampler.interval_noises.len(), 2);
        assert_eq!(sampler.interval_noises[0].len(), 6);
        assert_eq!(sampler.interval_noises[1].len(), 6);
    }

    #[test]
    fn resolve_model_dir_prefers_runtime_override() {
        let resolved = resolve_model_dir(Some(OsStr::new("/tmp/model-dir")));
        assert_eq!(resolved.unwrap(), PathBuf::from("/tmp/model-dir"));
    }

    #[test]
    fn resolve_heartmula_model_dir_prefers_runtime_override() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let expected = PathBuf::from(format!("/tmp/heartmula-{unique}"));
        unsafe {
            env::set_var("HEARTMULA_BURN_MODEL_DIR", &expected);
        }
        let resolved = resolve_heartmula_model_dir(None).expect("resolve");
        assert_eq!(resolved, expected);
        unsafe {
            env::remove_var("HEARTMULA_BURN_MODEL_DIR");
        }
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
            float_size: FloatSize::F16,
            heartmula_raw_bpk: root.join(heartmula_raw_bpk_rel(FloatSize::F16)),
            heartcodec_raw_bpk: root.join(heartcodec_raw_bpk_rel(FloatSize::F16)),
            tokenizer_json: root.join(HEARTMULA_TOKENIZER_REL),
            gen_config_json: root.join(HEARTMULA_GEN_CONFIG_REL),
        };
        let err = ensure_heartmula_model_paths(&paths).expect_err("missing files should error");
        let message = err.to_string();
        assert!(message.contains(heartmula_raw_bpk_rel(FloatSize::F16)));
        assert!(message.contains(heartcodec_raw_bpk_rel(FloatSize::F16)));
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
    fn default_hf_repo_cache_dir_uses_cache_root() {
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_else(|_| "/tmp".to_string());
        let repo_root = PathBuf::from(home)
            .join(".cache")
            .join("huggingface")
            .join("hub")
            .join("models--kurbloid--stable-audio-open-1.0-burn");
        assert_eq!(default_hf_repo_cache_dir(), repo_root);
    }
}
