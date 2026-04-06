use anyhow::{Context, Result, anyhow, bail};
use sentencepiece::SentencePieceProcessor;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub mod heartcodec;
pub mod heartmula_runtime;

pub const DEFAULT_MAX_PROMPT_TOKENS: usize = 128;
pub const DEFAULT_CFG_SCALE: f32 = 1.5;
pub const IPC_MODE_ENV: &str = "MAOLAN_BURN_SOCKETPAIR";

pub fn stderr_logging_enabled() -> bool {
    std::env::var_os(IPC_MODE_ENV).is_none()
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendChoice {
    Cpu,
    #[default]
    Vulkan,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ModelChoice {
    #[serde(rename = "happy-new-year")]
    #[default]
    HappyNewYear,
    #[serde(rename = "RL")]
    Rl,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GenerateRequest {
    #[serde(default)]
    pub model: ModelChoice,
    pub prompt: String,
    #[serde(default)]
    pub model_dir: Option<PathBuf>,
    #[serde(default = "default_output_path")]
    pub output_path: PathBuf,
    #[serde(default)]
    pub inspect_only: bool,
    pub backend: BackendChoice,
    pub cfg_scale: f32,
    #[serde(alias = "seconds_total", alias = "max_audio_length_ms")]
    pub length: usize,
    /// ODE steps for HeartMula flow matching (lower = faster, 10 = default)
    #[serde(default = "default_ode_steps")]
    pub ode_steps: usize,
    /// Lyrics prompt (alias for prompt)
    #[serde(default)]
    pub lyrics: Option<String>,
    /// Tags / style prompt
    #[serde(default)]
    pub tags: Option<String>,
    /// Top-k sampling for HeartMula token generation
    #[serde(default = "default_topk")]
    pub topk: usize,
    /// Sampling temperature for HeartMula token generation
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Decode an existing frames JSON instead of generating tokens
    #[serde(default)]
    pub decode_only: bool,
    /// Input frames JSON for decode-only mode
    #[serde(default)]
    pub frames_json: Option<PathBuf>,
    /// Number of worker threads to use for decode-only CPU decoding
    #[serde(default)]
    pub decode_threads: Option<usize>,
    /// Seed for deterministic HeartCodec decoder latent initialization
    #[serde(default)]
    pub decoder_seed: u64,
}

fn default_ode_steps() -> usize {
    10
}

fn default_topk() -> usize {
    50
}

fn default_temperature() -> f32 {
    1.0
}

pub type CliOptions = GenerateRequest;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GenerateResponseHeader {
    pub backend: BackendChoice,
    pub channels: usize,
    pub frames: usize,
    pub guidance_scale: f32,
    pub prompt_tokens: i64,
    pub sample_rate_hz: u32,
    pub length: usize,
    pub steps: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GenerateError {
    pub error: String,
}

/// Progress update message sent during generation
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GenerateProgress {
    pub phase: String, // "generator" or "decoder"
    pub progress: f32, // 0.0 to 1.0 within the phase
    pub operation: String,
}

fn default_output_path() -> PathBuf {
    PathBuf::from("output.wav")
}

pub fn help_text() -> &'static str {
    "\
maolan-generate

Usage:
  maolan-generate [options] <prompt-or-lyrics>

Options:
  --model <happy-new-year|RL>
  --model-dir <path>
  --output <path>
  --inspect
  --backend <cpu|vulkan>       Select the runtime backend
  --lyrics <text>          Prompt / lyrics (positional argument also accepted)
  --tags <text>            Style tags for HeartMula
  --cfg-scale <float>      CFG scale (1.0=no guidance, 2.0=weak, 6.0=strong)
  --length <int>           HeartMula: output length in milliseconds
  --topk <int>             HeartMula: top-k sampling (default: 50)
  --temperature <float>    HeartMula: sampling temperature (default: 1.0)
  --ode-steps <int>        HeartMula: flow matching steps (5=fast, 10=default, 20=best)
  --decoder-seed <int>     Seed for deterministic HeartCodec decoder latents
  --decode-only            Decode an existing frames JSON instead of generating tokens
  --frames-json <path>     Frames JSON input for --decode-only
  --decode-threads <int>    Number of worker threads for decode-only CPU decoding
  -h, --help
"
}

pub fn parse_options(args: impl IntoIterator<Item = OsString>) -> Result<CliOptions> {
    let mut args = args.into_iter();
    let _program = args.next();
    let mut prompt = None;
    let mut model_dir = None;
    let mut output_path = default_output_path();
    let mut inspect_only = false;
    let mut model = ModelChoice::HappyNewYear;
    let mut backend = BackendChoice::Vulkan;
    let mut cfg_scale = DEFAULT_CFG_SCALE;
    let mut length = 6_000_usize;
    let mut ode_steps = 10_usize;
    let mut lyrics = None;
    let mut tags = None;
    let mut topk = default_topk();
    let mut temperature = default_temperature();
    let mut decode_only = false;
    let mut frames_json = None;
    let mut decode_threads = None;
    let mut decoder_seed = 0_u64;

    while let Some(arg) = args.next() {
        let arg = arg
            .into_string()
            .map_err(|_| anyhow!("arguments must be valid UTF-8"))?;

        if matches!(arg.as_str(), "-h" | "--help") {
            bail!(help_text());
        }

        if arg == "--backend" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --backend"))?
                .into_string()
                .map_err(|_| anyhow!("backend value must be valid UTF-8"))?;
            backend = match value.as_str() {
                "cpu" => BackendChoice::Cpu,
                "vulkan" => BackendChoice::Vulkan,
                _ => bail!("unsupported backend '{value}', expected one of: cpu, vulkan"),
            };
            continue;
        }

        if arg == "--model-dir" {
            model_dir = Some(PathBuf::from(
                args.next()
                    .ok_or_else(|| anyhow!("missing value after --model-dir"))?,
            ));
            continue;
        }

        if arg == "--output" {
            output_path = PathBuf::from(
                args.next()
                    .ok_or_else(|| anyhow!("missing value after --output"))?,
            );
            continue;
        }

        if arg == "--lyrics" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --lyrics"))?
                .into_string()
                .map_err(|_| anyhow!("lyrics value must be valid UTF-8"))?;
            lyrics = Some(value);
            continue;
        }

        if arg == "--tags" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --tags"))?
                .into_string()
                .map_err(|_| anyhow!("tags value must be valid UTF-8"))?;
            tags = Some(value);
            continue;
        }

        if arg == "--length" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --length"))?
                .into_string()
                .map_err(|_| anyhow!("length value must be valid UTF-8"))?;
            length = value
                .parse::<usize>()
                .map_err(|_| anyhow!("length must be a whole number"))?;
            continue;
        }

        if arg == "--topk" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --topk"))?
                .into_string()
                .map_err(|_| anyhow!("topk value must be valid UTF-8"))?;
            topk = value
                .parse::<usize>()
                .map_err(|_| anyhow!("topk must be a whole number"))?;
            if topk == 0 {
                bail!("topk must be greater than zero");
            }
            continue;
        }

        if arg == "--temperature" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --temperature"))?
                .into_string()
                .map_err(|_| anyhow!("temperature value must be valid UTF-8"))?;
            temperature = value
                .parse::<f32>()
                .map_err(|_| anyhow!("temperature must be a number"))?;
            if !temperature.is_finite() || temperature < 0.0 {
                bail!("temperature must be a finite non-negative number");
            }
            continue;
        }

        if arg == "--inspect" {
            inspect_only = true;
            continue;
        }

        if arg == "--decode-only" {
            decode_only = true;
            continue;
        }

        if arg == "--frames-json" {
            frames_json =
                Some(PathBuf::from(args.next().ok_or_else(|| {
                    anyhow!("missing value after --frames-json")
                })?));
            continue;
        }

        if arg == "--decode-threads" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --decode-threads"))?
                .into_string()
                .map_err(|_| anyhow!("decode-threads value must be valid UTF-8"))?;
            decode_threads = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| anyhow!("decode-threads must be a whole number"))?,
            );
            continue;
        }

        if arg == "--decoder-seed" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --decoder-seed"))?
                .into_string()
                .map_err(|_| anyhow!("decoder-seed value must be valid UTF-8"))?;
            decoder_seed = value
                .parse::<u64>()
                .map_err(|_| anyhow!("decoder-seed must be a whole number"))?;
            continue;
        }

        if arg == "--model" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --model"))?
                .into_string()
                .map_err(|_| anyhow!("model value must be valid UTF-8"))?;
            model = match value.as_str() {
                "happy-new-year" => ModelChoice::HappyNewYear,
                "RL" => ModelChoice::Rl,
                _ => {
                    bail!("unsupported model '{value}', expected one of: happy-new-year, RL")
                }
            };
            continue;
        }

        if arg == "--cfg-scale" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --cfg-scale"))?
                .into_string()
                .map_err(|_| anyhow!("cfg-scale value must be valid UTF-8"))?;
            cfg_scale = value
                .parse::<f32>()
                .map_err(|_| anyhow!("cfg-scale must be a number"))?;
            if !cfg_scale.is_finite() || cfg_scale < 0.0 {
                bail!("cfg-scale must be a finite non-negative number");
            }
            continue;
        }

        if arg == "--ode-steps" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --ode-steps"))?
                .into_string()
                .map_err(|_| anyhow!("ode-steps value must be valid UTF-8"))?;
            ode_steps = value
                .parse::<usize>()
                .map_err(|_| anyhow!("ode-steps must be a whole number"))?;
            if ode_steps == 0 || ode_steps > 50 {
                bail!("ode-steps must be between 1 and 50");
            }
            continue;
        }

        if prompt.is_some() {
            bail!("expected exactly one positional argument: the prompt");
        }
        prompt = Some(arg);
    }

    let prompt = if decode_only {
        prompt.unwrap_or_default()
    } else if let Some(lyrics) = lyrics {
        lyrics
    } else {
        prompt.ok_or_else(|| {
            anyhow!("missing prompt argument; provide a positional argument or --lyrics")
        })?
    };
    let trimmed = prompt.trim();

    if !decode_only && trimmed.is_empty() {
        bail!("prompt argument cannot be empty");
    }

    validate_options(CliOptions {
        model,
        prompt: trimmed.to_owned(),
        model_dir,
        output_path,
        inspect_only,
        backend,
        cfg_scale,
        length,
        ode_steps,
        lyrics: None,
        tags,
        topk,
        temperature,
        decode_only,
        frames_json,
        decode_threads,
        decoder_seed,
    })
}

pub fn validate_options(mut options: CliOptions) -> Result<CliOptions> {
    let prompt = options.prompt.trim();
    if prompt.is_empty() && !options.decode_only {
        bail!("prompt argument cannot be empty");
    }
    options.prompt = prompt.to_owned();

    options.tags = options
        .tags
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    options.model_dir = options
        .model_dir
        .as_deref()
        .map(Path::new)
        .map(Path::to_path_buf);

    if !options.cfg_scale.is_finite() || options.cfg_scale < 0.0 {
        bail!("cfg-scale must be a finite non-negative number");
    }
    if options.length == 0 {
        bail!("length must be greater than zero");
    }
    if options.output_path.as_os_str().is_empty() {
        bail!("output path cannot be empty");
    }
    if options.decode_only && options.frames_json.is_none() {
        bail!("--decode-only requires --frames-json");
    }
    if options.frames_json.is_some() && !options.decode_only {
        bail!("--frames-json can only be used with --decode-only");
    }
    if let Some(threads) = options.decode_threads
        && threads == 0
    {
        bail!("--decode-threads must be greater than zero");
    }

    Ok(options)
}

pub fn read_ipc_message<T: DeserializeOwned>(reader: &mut impl Read) -> Result<T> {
    let mut len_bytes = [0_u8; 8];
    reader
        .read_exact(&mut len_bytes)
        .context("failed to read IPC message length")?;
    let len = u64::from_le_bytes(len_bytes);
    let len = usize::try_from(len).context("IPC message length is too large")?;
    let mut payload = vec![0_u8; len];
    reader
        .read_exact(&mut payload)
        .context("failed to read IPC message payload")?;
    serde_json::from_slice(&payload).context("failed to decode IPC JSON message")
}

pub fn write_ipc_message<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<()> {
    let payload = serde_json::to_vec(value).context("failed to encode IPC JSON message")?;
    let len = u64::try_from(payload.len()).context("IPC payload is too large")?;
    writer
        .write_all(&len.to_le_bytes())
        .context("failed to write IPC message length")?;
    writer
        .write_all(&payload)
        .context("failed to write IPC message payload")?;
    writer.flush().context("failed to flush IPC JSON message")?;
    Ok(())
}

pub fn write_ipc_bytes(writer: &mut impl Write, bytes: &[u8]) -> Result<()> {
    let len = u64::try_from(bytes.len()).context("IPC byte payload is too large")?;
    writer
        .write_all(&len.to_le_bytes())
        .context("failed to write IPC byte length")?;
    writer
        .write_all(bytes)
        .context("failed to write IPC byte payload")?;
    writer.flush().context("failed to flush IPC byte payload")?;
    Ok(())
}

pub fn tokenizer_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("t5-base-spiece.model")
}

pub fn load_tokenizer() -> Result<SentencePieceProcessor> {
    SentencePieceProcessor::open(tokenizer_path())
        .context("failed to open the bundled T5 sentencepiece model")
}

pub fn encode_prompt(
    tokenizer: &SentencePieceProcessor,
    prompt: &str,
    max_tokens: usize,
) -> Result<(Vec<i64>, Vec<i64>)> {
    let mut token_ids = Vec::with_capacity(max_tokens);

    if let Some(bos_id) = tokenizer.bos_id() {
        token_ids.push(i64::from(bos_id));
    }

    for piece in tokenizer
        .encode(prompt)
        .context("failed to tokenize prompt")?
    {
        if token_ids.len() >= max_tokens {
            break;
        }
        token_ids.push(i64::from(piece.id));
    }

    if token_ids.len() < max_tokens
        && let Some(eos_id) = tokenizer.eos_id()
    {
        token_ids.push(i64::from(eos_id));
    }

    if token_ids.len() > max_tokens {
        token_ids.truncate(max_tokens);
    }

    let attention_len = token_ids.len();
    let mut attention_mask = vec![1_i64; attention_len];
    token_ids.resize(max_tokens, 0);
    attention_mask.resize(max_tokens, 0);

    Ok((token_ids, attention_mask))
}

#[cfg(test)]
mod tests {
    use super::{BackendChoice, DEFAULT_MAX_PROMPT_TOKENS, ModelChoice, parse_options};
    use std::ffi::OsString;

    #[test]
    fn parses_single_prompt_argument() {
        let args = [OsString::from("generate"), OsString::from("warm tape hiss")];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.prompt, "warm tape hiss");
        assert_eq!(options.model, ModelChoice::HappyNewYear);
        assert_eq!(options.backend, BackendChoice::Vulkan);
        assert_eq!(options.cfg_scale, 1.5);
        assert_eq!(options.length, 6_000);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let args = [
            OsString::from("generate"),
            OsString::from("  foley footsteps  "),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.prompt, "foley footsteps");
    }

    #[test]
    fn rejects_missing_prompt() {
        let args = [OsString::from("generate")];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn parses_backend_flag_after_prompt() {
        let args = [
            OsString::from("generate"),
            OsString::from("warm tape hiss"),
            OsString::from("--backend"),
            OsString::from("vulkan"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.backend, BackendChoice::Vulkan);
    }

    #[test]
    fn parses_model_flag() {
        let args = [
            OsString::from("generate"),
            OsString::from("--model"),
            OsString::from("happy-new-year"),
            OsString::from("verse and chorus"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.model, ModelChoice::HappyNewYear);
    }

    #[test]
    fn parses_rl_model_flag() {
        let args = [
            OsString::from("generate"),
            OsString::from("--model"),
            OsString::from("RL"),
            OsString::from("verse and chorus"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.model, ModelChoice::Rl);
    }

    #[test]
    fn parses_tags_cfg_and_length() {
        let args = [
            OsString::from("generate"),
            OsString::from("--tags"),
            OsString::from("warm tape hiss"),
            OsString::from("--cfg-scale"),
            OsString::from("4.5"),
            OsString::from("--ode-steps"),
            OsString::from("20"),
            OsString::from("--length"),
            OsString::from("8000"),
            OsString::from("verse and chorus"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.cfg_scale, 4.5);
        assert_eq!(options.ode_steps, 20);
        assert_eq!(options.length, 8_000);
    }

    #[test]
    fn parses_decode_only_without_prompt() {
        let args = [
            OsString::from("generate"),
            OsString::from("--decode-only"),
            OsString::from("--frames-json"),
            OsString::from("/tmp/frames.json"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert!(options.decode_only);
        assert_eq!(
            options.frames_json.as_deref(),
            Some(std::path::Path::new("/tmp/frames.json"))
        );
        assert!(options.prompt.is_empty());
    }

    #[test]
    fn parses_decode_threads() {
        let args = [
            OsString::from("generate"),
            OsString::from("--decode-only"),
            OsString::from("--frames-json"),
            OsString::from("/tmp/frames.json"),
            OsString::from("--decode-threads"),
            OsString::from("8"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.decode_threads, Some(8));
    }

    const _: () = assert!(DEFAULT_MAX_PROMPT_TOKENS == 128);
}
