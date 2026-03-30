use anyhow::{Context, Result, anyhow};
use burn::prelude::Backend;
use burn::tensor::{Int, Tensor, TensorData};
use burn_store::{BurnpackStore, ModuleSnapshot};
use maolan_burn::{
    BackendChoice, DEFAULT_MAX_PROMPT_TOKENS, GenerateResponseHeader, SamplerChoice, encode_prompt,
    load_tokenizer, parse_options, read_ipc_message, validate_options, write_ipc_bytes,
    write_ipc_message,
};
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::env;
use std::f32::consts::PI;
use std::io;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/model_bindings.rs"));

const LATENT_CHANNELS: usize = 64;
const LATENT_LENGTH: usize = 256;
const AUDIO_SAMPLE_RATE: usize = 44_100;
const SIGMA_MIN: f32 = 0.03;
const SIGMA_MAX: f32 = 1000.0;
const SIGMA_RHO: f32 = 1.0;
const DEFAULT_ETA: f32 = 1.0;
const DEFAULT_S_NOISE: f32 = 1.0;
const DEFAULT_OUTPUT_PATH: &str = "output.wav";
const DEFAULT_SEED: u64 = 0;
const DEFAULT_SECONDS_START: i64 = 0;
const IPC_MODE_ENV: &str = "MAOLAN_BURN_SOCKETPAIR";
const MODEL_DIR_ENV: &str = "MAOLAN_BURN_MODEL_DIR";
const DEFAULT_RUNTIME_MODEL_REPO_DIR: &str =
    ".cache/huggingface/hub/models--kurbloid--stable-audio-open-1.0-burn";
const T5_BPK_REL: &str = "burn_t5/stable_audio_t5_sim.bpk";
const DIT_BPK_REL: &str = "burn_dit/stable_audio_dit.bpk";
const VAE_BPK_REL: &str = "burn_vae/stable_audio_vae_decoder_sim.bpk";

struct RuntimeModelPaths {
    model_dir: PathBuf,
    t5_bpk: String,
    dit_bpk: String,
    vae_bpk: String,
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

    let options = parse_options(env::args_os())?;
    let model_paths = resolve_runtime_model_paths()?;
    eprintln!(
        "maolan-burn: mode=cli backend={} sampler={} model_dir={}",
        backend_name(options.backend),
        sampler_name(options.sampler),
        model_paths.model_dir.display()
    );

    match options.backend {
        BackendChoice::Cpu => run_cpu(&options),
        BackendChoice::Vulkan => run_vulkan(&options),
        BackendChoice::Cuda => run_cuda(&options),
    }
}

fn run_ipc() -> Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let options = validate_options(read_ipc_message(&mut stdin)?)?;
    let model_paths = resolve_runtime_model_paths()?;
    eprintln!(
        "maolan-burn: mode=ipc backend={} sampler={} model_dir={}",
        backend_name(options.backend),
        sampler_name(options.sampler),
        model_paths.model_dir.display()
    );
    let output = match options.backend {
        BackendChoice::Cpu => generate_cpu_output(&options),
        BackendChoice::Vulkan => generate_vulkan_output(&options),
        BackendChoice::Cuda => generate_cuda_output(&options),
    }?;
    write_ipc_message(&mut stdout, &output.header)?;
    write_ipc_bytes(&mut stdout, &output.wav_bytes)?;
    eprintln!(
        "maolan-burn: mode=ipc complete backend={} wav_bytes={}",
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
    let mut store = BurnpackStore::from_file(&model_paths.t5_bpk);
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
    let mut store = BurnpackStore::from_file(&model_paths.dit_bpk);
    model.load_from(&mut store).with_context(|| {
        format!(
            "failed to load DiT weights from {}. The runtime .bpk files may not match the compiled generated sources from {}",
            model_paths.dit_bpk, GENERATED_SOURCE_DIR
        )
    })?;
    Ok(model)
}

fn run_with_backend<B>(
    options: &maolan_burn::CliOptions,
    backend: BackendChoice,
) -> Result<GeneratedOutput>
where
    B: Backend,
    B::Device: Default,
{
    let target_frames = target_audio_frames(options.seconds_total);
    let model_paths = resolve_runtime_model_paths()?;
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

fn backend_name(backend: BackendChoice) -> &'static str {
    match backend {
        BackendChoice::Cpu => "cpu",
        BackendChoice::Vulkan => "vulkan",
        BackendChoice::Cuda => "cuda",
    }
}

fn write_cli_output(
    options: &maolan_burn::CliOptions,
    backend_label: &str,
    output: &GeneratedOutput,
) -> Result<()> {
    let model_paths = resolve_runtime_model_paths()?;
    write_wav(DEFAULT_OUTPUT_PATH, &output.wav_bytes)?;

    println!("maolan-burn");
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
    println!("output={}", Path::new(DEFAULT_OUTPUT_PATH).display());
    eprintln!(
        "maolan-burn: mode=cli complete backend={} output={} wav_bytes={}",
        backend_label,
        Path::new(DEFAULT_OUTPUT_PATH).display(),
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

fn write_wav(path: &str, wav_bytes: &[u8]) -> Result<()> {
    std::fs::write(path, wav_bytes)
        .with_context(|| format!("failed to write '{}'", Path::new(path).display()))?;
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
    let mut store = BurnpackStore::from_file(vae_bpk);
    model.load_from(&mut store).with_context(|| {
        format!(
            "failed to load VAE weights from {}. The runtime .bpk files may not match the compiled generated sources from {}",
            vae_bpk, GENERATED_SOURCE_DIR
        )
    })?;
    Ok(model.forward(latent))
}

fn resolve_runtime_model_paths() -> Result<RuntimeModelPaths> {
    let model_dir = resolve_model_dir(env::var_os(MODEL_DIR_ENV).as_deref())?;
    Ok(RuntimeModelPaths {
        t5_bpk: model_dir.join(T5_BPK_REL).display().to_string(),
        dit_bpk: model_dir.join(DIT_BPK_REL).display().to_string(),
        vae_bpk: model_dir.join(VAE_BPK_REL).display().to_string(),
        model_dir,
    })
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
            "failed to locate maolan-burn model snapshot in Hugging Face hub cache; set {MODEL_DIR_ENV} or download the model via maolan"
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

#[cfg(feature = "backend-ndarray")]
fn generate_cpu_output(options: &maolan_burn::CliOptions) -> Result<GeneratedOutput> {
    run_with_backend::<burn::backend::NdArray<f32>>(options, BackendChoice::Cpu)
}

#[cfg(not(feature = "backend-ndarray"))]
fn generate_cpu_output(_options: &maolan_burn::CliOptions) -> Result<GeneratedOutput> {
    anyhow::bail!("the cpu backend was not compiled in; rebuild with --features backend-ndarray")
}

#[cfg(feature = "backend-ndarray")]
fn run_cpu(options: &maolan_burn::CliOptions) -> Result<()> {
    let output = generate_cpu_output(options)?;
    write_cli_output(options, "cpu", &output)
}

#[cfg(not(feature = "backend-ndarray"))]
fn run_cpu(_options: &maolan_burn::CliOptions) -> Result<()> {
    anyhow::bail!("the cpu backend was not compiled in; rebuild with --features backend-ndarray")
}

#[cfg(feature = "backend-vulkan")]
fn generate_vulkan_output(options: &maolan_burn::CliOptions) -> Result<GeneratedOutput> {
    let device = burn::backend::wgpu::WgpuDevice::default();
    burn::backend::wgpu::init_setup::<burn::backend::wgpu::graphics::Vulkan>(
        &device,
        Default::default(),
    );
    run_with_backend::<burn::backend::Wgpu<f32, i64, u32>>(options, BackendChoice::Vulkan)
}

#[cfg(not(feature = "backend-vulkan"))]
fn generate_vulkan_output(_options: &maolan_burn::CliOptions) -> Result<GeneratedOutput> {
    anyhow::bail!("the vulkan backend was not compiled in; rebuild with --features backend-vulkan")
}

#[cfg(feature = "backend-vulkan")]
fn run_vulkan(options: &maolan_burn::CliOptions) -> Result<()> {
    let output = generate_vulkan_output(options)?;
    write_cli_output(options, "vulkan", &output)
}

#[cfg(not(feature = "backend-vulkan"))]
fn run_vulkan(_options: &maolan_burn::CliOptions) -> Result<()> {
    anyhow::bail!("the vulkan backend was not compiled in; rebuild with --features backend-vulkan")
}

#[cfg(feature = "backend-cuda")]
fn generate_cuda_output(options: &maolan_burn::CliOptions) -> Result<GeneratedOutput> {
    run_with_backend::<burn::backend::Cuda<f32, i64>>(options, BackendChoice::Cuda)
}

#[cfg(not(feature = "backend-cuda"))]
fn generate_cuda_output(_options: &maolan_burn::CliOptions) -> Result<GeneratedOutput> {
    anyhow::bail!("the cuda backend was not compiled in; rebuild with --features backend-cuda")
}

#[cfg(feature = "backend-cuda")]
fn run_cuda(options: &maolan_burn::CliOptions) -> Result<()> {
    let output = generate_cuda_output(options)?;
    write_cli_output(options, "cuda", &output)
}

#[cfg(not(feature = "backend-cuda"))]
fn run_cuda(_options: &maolan_burn::CliOptions) -> Result<()> {
    anyhow::bail!("the cuda backend was not compiled in; rebuild with --features backend-cuda")
}

#[cfg(test)]
mod tests {
    use super::{
        BrownianNoiseSampler, default_hf_repo_cache_dir, normalize_audio_peak,
        polyexponential_sigmas, resolve_model_dir, sampler_name, sigma_to_t, target_audio_frames,
        trim_audio_frames,
    };
    use maolan_burn::SamplerChoice;
    use rand::{SeedableRng, rngs::SmallRng};
    use std::{env, ffi::OsStr, path::PathBuf};

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
