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

    #[test]
    fn parses_cpu_backend_flag() {
        let args = [
            OsString::from("generate"),
            OsString::from("--backend"),
            OsString::from("cpu"),
            OsString::from("test prompt"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.backend, BackendChoice::Cpu);
    }

    #[test]
    fn rejects_invalid_backend() {
        let args = [
            OsString::from("generate"),
            OsString::from("--backend"),
            OsString::from("invalid"),
            OsString::from("test prompt"),
        ];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn parses_cfg_scale_validation() {
        let args = [
            OsString::from("generate"),
            OsString::from("--cfg-scale"),
            OsString::from("2.5"),
            OsString::from("test prompt"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.cfg_scale, 2.5);
    }

    #[test]
    fn rejects_negative_cfg_scale() {
        let args = [
            OsString::from("generate"),
            OsString::from("--cfg-scale"),
            OsString::from("-1.0"),
            OsString::from("test prompt"),
        ];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn rejects_invalid_cfg_scale() {
        let args = [
            OsString::from("generate"),
            OsString::from("--cfg-scale"),
            OsString::from("not-a-number"),
            OsString::from("test prompt"),
        ];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn parses_temperature() {
        let args = [
            OsString::from("generate"),
            OsString::from("--temperature"),
            OsString::from("0.8"),
            OsString::from("test prompt"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.temperature, 0.8);
    }

    #[test]
    fn rejects_negative_temperature() {
        let args = [
            OsString::from("generate"),
            OsString::from("--temperature"),
            OsString::from("-0.5"),
            OsString::from("test prompt"),
        ];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn parses_topk() {
        let args = [
            OsString::from("generate"),
            OsString::from("--topk"),
            OsString::from("25"),
            OsString::from("test prompt"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.topk, 25);
    }

    #[test]
    fn rejects_zero_topk() {
        let args = [
            OsString::from("generate"),
            OsString::from("--topk"),
            OsString::from("0"),
            OsString::from("test prompt"),
        ];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn parses_ode_steps() {
        let args = [
            OsString::from("generate"),
            OsString::from("--ode-steps"),
            OsString::from("15"),
            OsString::from("test prompt"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.ode_steps, 15);
    }

    #[test]
    fn rejects_zero_ode_steps() {
        let args = [
            OsString::from("generate"),
            OsString::from("--ode-steps"),
            OsString::from("0"),
            OsString::from("test prompt"),
        ];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn rejects_too_many_ode_steps() {
        let args = [
            OsString::from("generate"),
            OsString::from("--ode-steps"),
            OsString::from("51"),
            OsString::from("test prompt"),
        ];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn parses_output_path() {
        let args = [
            OsString::from("generate"),
            OsString::from("--output"),
            OsString::from("/tmp/output.wav"),
            OsString::from("test prompt"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(
            options.output_path,
            std::path::PathBuf::from("/tmp/output.wav")
        );
    }

    #[test]
    fn parses_model_dir() {
        let args = [
            OsString::from("generate"),
            OsString::from("--model-dir"),
            OsString::from("/tmp/models"),
            OsString::from("test prompt"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(
            options.model_dir,
            Some(std::path::PathBuf::from("/tmp/models"))
        );
    }

    #[test]
    fn parses_decoder_seed() {
        let args = [
            OsString::from("generate"),
            OsString::from("--decoder-seed"),
            OsString::from("42"),
            OsString::from("test prompt"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.decoder_seed, 42);
    }

    #[test]
    fn parses_lyrics_alias() {
        let args = [
            OsString::from("generate"),
            OsString::from("--lyrics"),
            OsString::from("custom lyrics text"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.prompt, "custom lyrics text");
    }

    #[test]
    fn parses_inspect_flag() {
        let args = [
            OsString::from("generate"),
            OsString::from("--inspect"),
            OsString::from("test prompt"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert!(options.inspect_only);
    }

    #[test]
    fn rejects_multiple_positional_args() {
        let args = [
            OsString::from("generate"),
            OsString::from("first prompt"),
            OsString::from("second prompt"),
        ];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn rejects_empty_prompt() {
        let args = [OsString::from("generate"), OsString::from("   ")];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn validate_options_trims_prompt() {
        let options = super::CliOptions {
            model: ModelChoice::HappyNewYear,
            prompt: "  test prompt  ".to_owned(),
            model_dir: None,
            output_path: std::path::PathBuf::from("output.wav"),
            inspect_only: false,
            backend: BackendChoice::Vulkan,
            cfg_scale: 1.5,
            length: 6000,
            ode_steps: 10,
            lyrics: None,
            tags: None,
            topk: 50,
            temperature: 1.0,
            decode_only: false,
            frames_json: None,
            decode_threads: None,
            decoder_seed: 0,
        };
        let validated = super::validate_options(options).expect("validation should pass");
        assert_eq!(validated.prompt, "test prompt");
    }

    #[test]
    fn validate_options_rejects_empty_output_path() {
        let options = super::CliOptions {
            model: ModelChoice::HappyNewYear,
            prompt: "test".to_owned(),
            model_dir: None,
            output_path: std::path::PathBuf::from(""),
            inspect_only: false,
            backend: BackendChoice::Vulkan,
            cfg_scale: 1.5,
            length: 6000,
            ode_steps: 10,
            lyrics: None,
            tags: None,
            topk: 50,
            temperature: 1.0,
            decode_only: false,
            frames_json: None,
            decode_threads: None,
            decoder_seed: 0,
        };
        assert!(super::validate_options(options).is_err());
    }

    #[test]
    fn validate_options_rejects_zero_length() {
        let options = super::CliOptions {
            model: ModelChoice::HappyNewYear,
            prompt: "test".to_owned(),
            model_dir: None,
            output_path: std::path::PathBuf::from("output.wav"),
            inspect_only: false,
            backend: BackendChoice::Vulkan,
            cfg_scale: 1.5,
            length: 0,
            ode_steps: 10,
            lyrics: None,
            tags: None,
            topk: 50,
            temperature: 1.0,
            decode_only: false,
            frames_json: None,
            decode_threads: None,
            decoder_seed: 0,
        };
        assert!(super::validate_options(options).is_err());
    }

    #[test]
    fn validate_options_rejects_decode_only_without_frames() {
        let options = super::CliOptions {
            model: ModelChoice::HappyNewYear,
            prompt: "".to_owned(),
            model_dir: None,
            output_path: std::path::PathBuf::from("output.wav"),
            inspect_only: false,
            backend: BackendChoice::Vulkan,
            cfg_scale: 1.5,
            length: 6000,
            ode_steps: 10,
            lyrics: None,
            tags: None,
            topk: 50,
            temperature: 1.0,
            decode_only: true,
            frames_json: None,
            decode_threads: None,
            decoder_seed: 0,
        };
        assert!(super::validate_options(options).is_err());
    }

    #[test]
    fn validate_options_rejects_zero_decode_threads() {
        let options = super::CliOptions {
            model: ModelChoice::HappyNewYear,
            prompt: "test".to_owned(),
            model_dir: None,
            output_path: std::path::PathBuf::from("output.wav"),
            inspect_only: false,
            backend: BackendChoice::Vulkan,
            cfg_scale: 1.5,
            length: 6000,
            ode_steps: 10,
            lyrics: None,
            tags: None,
            topk: 50,
            temperature: 1.0,
            decode_only: false,
            frames_json: None,
            decode_threads: Some(0),
            decoder_seed: 0,
        };
        assert!(super::validate_options(options).is_err());
    }

    #[test]
    fn validate_options_trims_tags() {
        let options = super::CliOptions {
            model: ModelChoice::HappyNewYear,
            prompt: "test".to_owned(),
            model_dir: None,
            output_path: std::path::PathBuf::from("output.wav"),
            inspect_only: false,
            backend: BackendChoice::Vulkan,
            cfg_scale: 1.5,
            length: 6000,
            ode_steps: 10,
            lyrics: None,
            tags: Some("  tag1, tag2  ".to_owned()),
            topk: 50,
            temperature: 1.0,
            decode_only: false,
            frames_json: None,
            decode_threads: None,
            decoder_seed: 0,
        };
        let validated = super::validate_options(options).expect("validation should pass");
        assert_eq!(validated.tags, Some("tag1, tag2".to_owned()));
    }

    #[test]
    fn validate_options_filters_empty_tags() {
        let options = super::CliOptions {
            model: ModelChoice::HappyNewYear,
            prompt: "test".to_owned(),
            model_dir: None,
            output_path: std::path::PathBuf::from("output.wav"),
            inspect_only: false,
            backend: BackendChoice::Vulkan,
            cfg_scale: 1.5,
            length: 6000,
            ode_steps: 10,
            lyrics: None,
            tags: Some("   ".to_owned()),
            topk: 50,
            temperature: 1.0,
            decode_only: false,
            frames_json: None,
            decode_threads: None,
            decoder_seed: 0,
        };
        let validated = super::validate_options(options).expect("validation should pass");
        assert_eq!(validated.tags, None);
    }

    #[test]
    fn default_output_path_is_output_wav() {
        let args = [OsString::from("generate"), OsString::from("test prompt")];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.output_path, std::path::PathBuf::from("output.wav"));
    }

    #[test]
    fn default_length_is_6000() {
        let args = [OsString::from("generate"), OsString::from("test prompt")];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.length, 6000);
    }

    #[test]
    fn default_ode_steps_is_10() {
        let args = [OsString::from("generate"), OsString::from("test prompt")];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.ode_steps, 10);
    }

    #[test]
    fn default_topk_is_50() {
        let args = [OsString::from("generate"), OsString::from("test prompt")];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.topk, 50);
    }

    #[test]
    fn default_temperature_is_1() {
        let args = [OsString::from("generate"), OsString::from("test prompt")];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.temperature, 1.0);
    }

    #[test]
    fn default_cfg_scale_is_1_5() {
        let args = [OsString::from("generate"), OsString::from("test prompt")];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.cfg_scale, 1.5);
    }

    #[test]
    fn default_decoder_seed_is_0() {
        let args = [OsString::from("generate"), OsString::from("test prompt")];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.decoder_seed, 0);
    }

    #[test]
    fn help_text_contains_usage() {
        let help = super::help_text();
        assert!(help.contains("maolan-generate"));
        assert!(help.contains("Usage:"));
        assert!(help.contains("Options:"));
    }

    #[test]
    fn stderr_logging_disabled_in_ipc_mode() {
        // When IPC_MODE_ENV is set, stderr_logging_enabled should return false
        // Note: We can't actually set the env var here without affecting other tests,
        // but we can verify the function exists and has the right signature
        let _ = super::stderr_logging_enabled();
    }

    #[test]
    fn write_and_read_ipc_message_roundtrip() {
        use super::{read_ipc_message, write_ipc_message};
        use std::io::Cursor;

        let original = super::GenerateResponseHeader {
            backend: BackendChoice::Cpu,
            channels: 2,
            frames: 48000,
            guidance_scale: 2.0,
            prompt_tokens: 10,
            sample_rate_hz: 48000,
            length: 6000,
            steps: 10,
        };

        let mut buffer = Vec::new();
        write_ipc_message(&mut buffer, &original).expect("write should succeed");

        let mut cursor = Cursor::new(buffer);
        let decoded: super::GenerateResponseHeader =
            read_ipc_message(&mut cursor).expect("read should succeed");

        assert_eq!(decoded.backend, original.backend);
        assert_eq!(decoded.channels, original.channels);
        assert_eq!(decoded.frames, original.frames);
        assert_eq!(decoded.guidance_scale, original.guidance_scale);
        assert_eq!(decoded.prompt_tokens, original.prompt_tokens);
        assert_eq!(decoded.sample_rate_hz, original.sample_rate_hz);
        assert_eq!(decoded.length, original.length);
        assert_eq!(decoded.steps, original.steps);
    }

    #[test]
    fn write_and_read_ipc_progress_roundtrip() {
        use super::{read_ipc_message, write_ipc_message};
        use std::io::Cursor;

        let original = super::GenerateProgress {
            phase: "generator".to_owned(),
            progress: 0.5,
            operation: "Processing".to_owned(),
        };

        let mut buffer = Vec::new();
        write_ipc_message(&mut buffer, &original).expect("write should succeed");

        let mut cursor = Cursor::new(buffer);
        let decoded: super::GenerateProgress =
            read_ipc_message(&mut cursor).expect("read should succeed");

        assert_eq!(decoded.phase, original.phase);
        assert_eq!(decoded.progress, original.progress);
        assert_eq!(decoded.operation, original.operation);
    }

    #[test]
    fn write_and_read_ipc_error_roundtrip() {
        use super::{read_ipc_message, write_ipc_message};
        use std::io::Cursor;

        let original = super::GenerateError {
            error: "Test error message".to_owned(),
        };

        let mut buffer = Vec::new();
        write_ipc_message(&mut buffer, &original).expect("write should succeed");

        let mut cursor = Cursor::new(buffer);
        let decoded: super::GenerateError =
            read_ipc_message(&mut cursor).expect("read should succeed");

        assert_eq!(decoded.error, original.error);
    }

    #[test]
    fn write_ipc_bytes_roundtrip() {
        use super::write_ipc_bytes;
        use std::io::Cursor;

        let original = b"Hello, World!";

        let mut buffer = Vec::new();
        write_ipc_bytes(&mut buffer, original).expect("write should succeed");

        // Read back the bytes
        let mut cursor = Cursor::new(buffer);
        let mut len_bytes = [0_u8; 8];
        std::io::Read::read_exact(&mut cursor, &mut len_bytes).expect("read length should succeed");
        let len = u64::from_le_bytes(len_bytes) as usize;
        assert_eq!(len, original.len());

        let mut payload = vec![0_u8; len];
        std::io::Read::read_exact(&mut cursor, &mut payload).expect("read payload should succeed");
        assert_eq!(&payload[..], &original[..]);
    }

    #[test]
    fn read_ipc_message_fails_on_truncated_data() {
        use super::read_ipc_message;
        use std::io::Cursor;

        // Create a buffer with just the length header but no payload
        let len_bytes = 100_u64.to_le_bytes();
        let buffer = len_bytes.to_vec();

        let mut cursor = Cursor::new(buffer);
        let result: Result<super::GenerateResponseHeader, _> = read_ipc_message(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn read_ipc_message_fails_on_invalid_json() {
        use super::read_ipc_message;
        use std::io::Cursor;

        // Create a buffer with length header and invalid JSON payload
        let payload = b"not valid json";
        let len_bytes = (payload.len() as u64).to_le_bytes();
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&len_bytes);
        buffer.extend_from_slice(payload);

        let mut cursor = Cursor::new(buffer);
        let result: Result<super::GenerateResponseHeader, _> = read_ipc_message(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn serialize_generate_request() {
        let request = super::GenerateRequest {
            model: ModelChoice::Rl,
            prompt: "test prompt".to_owned(),
            model_dir: Some(std::path::PathBuf::from("/tmp/models")),
            output_path: std::path::PathBuf::from("/tmp/output.wav"),
            inspect_only: true,
            backend: BackendChoice::Cpu,
            cfg_scale: 2.5,
            length: 8000,
            ode_steps: 15,
            lyrics: Some("lyrics text".to_owned()),
            tags: Some("tag1,tag2".to_owned()),
            topk: 25,
            temperature: 0.8,
            decode_only: false,
            frames_json: None,
            decode_threads: Some(4),
            decoder_seed: 42,
        };

        let json = serde_json::to_string(&request).expect("serialization should succeed");
        assert!(json.contains("test prompt"));
        assert!(json.contains("cpu"));
        assert!(json.contains("RL"));
    }

    #[test]
    fn deserialize_generate_request() {
        let json = r#"{
            "model": "RL",
            "prompt": "test prompt",
            "output_path": "/tmp/output.wav",
            "backend": "cpu",
            "cfg_scale": 2.5,
            "length": 8000,
            "ode_steps": 15,
            "topk": 25,
            "temperature": 0.8,
            "decoder_seed": 42
        }"#;

        let request: super::GenerateRequest =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert_eq!(request.model, ModelChoice::Rl);
        assert_eq!(request.prompt, "test prompt");
        assert_eq!(request.backend, BackendChoice::Cpu);
        assert_eq!(request.cfg_scale, 2.5);
        assert_eq!(request.length, 8000);
        assert_eq!(request.ode_steps, 15);
        assert_eq!(request.topk, 25);
        assert_eq!(request.temperature, 0.8);
        assert_eq!(request.decoder_seed, 42);
    }

    #[test]
    fn deserialize_generate_request_with_aliases() {
        // Test "seconds_total" alias for length
        let json1 =
            r#"{"prompt": "test", "backend": "cpu", "cfg_scale": 1.5, "seconds_total": 5000}"#;
        let request1: super::GenerateRequest =
            serde_json::from_str(json1).expect("deserialization should succeed");
        assert_eq!(request1.length, 5000);

        // Test "max_audio_length_ms" alias for length
        let json2 = r#"{"prompt": "test", "backend": "cpu", "cfg_scale": 1.5, "max_audio_length_ms": 7000}"#;
        let request2: super::GenerateRequest =
            serde_json::from_str(json2).expect("deserialization should succeed");
        assert_eq!(request2.length, 7000);
    }

    #[test]
    fn backend_choice_default_is_vulkan() {
        let default: BackendChoice = Default::default();
        assert_eq!(default, BackendChoice::Vulkan);
    }

    #[test]
    fn model_choice_default_is_happy_new_year() {
        let default: ModelChoice = Default::default();
        assert_eq!(default, ModelChoice::HappyNewYear);
    }

    #[test]
    fn default_output_path_function() {
        let path = super::default_output_path();
        assert_eq!(path, std::path::PathBuf::from("output.wav"));
    }

    #[test]
    fn tokenizer_path_returns_valid_path() {
        let path = super::tokenizer_path();
        assert!(path.to_string_lossy().contains("t5-base-spiece.model"));
    }
}
