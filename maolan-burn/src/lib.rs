use anyhow::{Context, Result, anyhow, bail};
use sentencepiece::SentencePieceProcessor;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const DEFAULT_MAX_PROMPT_TOKENS: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendChoice {
    Cpu,
    Vulkan,
    Cuda,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SamplerChoice {
    #[serde(rename = "dpmpp-2m")]
    Dpmpp2m,
    #[serde(rename = "dpmpp-3m-sde")]
    Dpmpp3mSde,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GenerateRequest {
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub backend: BackendChoice,
    pub sampler: SamplerChoice,
    pub cfg_scale: f32,
    pub steps: usize,
    pub seconds_total: i64,
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
    pub sampler: SamplerChoice,
    pub seconds_total: i64,
    pub steps: usize,
    pub wav_bytes_len: usize,
}

pub fn parse_options(args: impl IntoIterator<Item = OsString>) -> Result<CliOptions> {
    let mut args = args.into_iter();
    let _program = args.next();
    let mut prompt = None;
    let mut negative_prompt = None;
    let mut backend = BackendChoice::Cpu;
    let mut sampler = SamplerChoice::Dpmpp3mSde;
    let mut cfg_scale = 6.0_f32;
    let mut steps = 250_usize;
    let mut seconds_total = 6_i64;

    while let Some(arg) = args.next() {
        let arg = arg
            .into_string()
            .map_err(|_| anyhow!("arguments must be valid UTF-8"))?;

        if arg == "--backend" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --backend"))?
                .into_string()
                .map_err(|_| anyhow!("backend value must be valid UTF-8"))?;
            backend = match value.as_str() {
                "cpu" => BackendChoice::Cpu,
                "vulkan" => BackendChoice::Vulkan,
                "cuda" => BackendChoice::Cuda,
                _ => bail!("unsupported backend '{value}', expected one of: cpu, vulkan, cuda"),
            };
            continue;
        }

        if arg == "--sampler" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --sampler"))?
                .into_string()
                .map_err(|_| anyhow!("sampler value must be valid UTF-8"))?;
            sampler = match value.as_str() {
                "dpmpp-2m" => SamplerChoice::Dpmpp2m,
                "dpmpp-3m-sde" => SamplerChoice::Dpmpp3mSde,
                _ => {
                    bail!("unsupported sampler '{value}', expected one of: dpmpp-2m, dpmpp-3m-sde")
                }
            };
            continue;
        }

        if arg == "--negative-prompt" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --negative-prompt"))?
                .into_string()
                .map_err(|_| anyhow!("negative prompt must be valid UTF-8"))?;
            let trimmed = value.trim();
            negative_prompt = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
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

        if arg == "--steps" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --steps"))?
                .into_string()
                .map_err(|_| anyhow!("steps value must be valid UTF-8"))?;
            steps = value
                .parse::<usize>()
                .map_err(|_| anyhow!("steps must be a whole number"))?;
            if steps == 0 {
                bail!("steps must be greater than zero");
            }
            continue;
        }

        if arg == "--seconds-total" {
            let value = args
                .next()
                .ok_or_else(|| anyhow!("missing value after --seconds-total"))?
                .into_string()
                .map_err(|_| anyhow!("seconds-total value must be valid UTF-8"))?;
            seconds_total = value
                .parse::<i64>()
                .map_err(|_| anyhow!("seconds-total must be a whole number"))?;
            continue;
        }

        if prompt.is_some() {
            bail!("expected exactly one positional argument: the prompt");
        }
        prompt = Some(arg);
    }

    let prompt = prompt.ok_or_else(|| anyhow!("missing prompt argument"))?;
    let trimmed = prompt.trim();

    if trimmed.is_empty() {
        bail!("prompt argument cannot be empty");
    }

    validate_options(CliOptions {
        prompt: trimmed.to_owned(),
        negative_prompt,
        backend,
        sampler,
        cfg_scale,
        steps,
        seconds_total,
    })
}

pub fn validate_options(mut options: CliOptions) -> Result<CliOptions> {
    let prompt = options.prompt.trim();
    if prompt.is_empty() {
        bail!("prompt argument cannot be empty");
    }
    options.prompt = prompt.to_owned();

    options.negative_prompt = options
        .negative_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    if !options.cfg_scale.is_finite() || options.cfg_scale < 0.0 {
        bail!("cfg-scale must be a finite non-negative number");
    }
    if options.steps == 0 {
        bail!("steps must be greater than zero");
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
    use super::{BackendChoice, DEFAULT_MAX_PROMPT_TOKENS, SamplerChoice, parse_options};
    use std::ffi::OsString;

    #[test]
    fn parses_single_prompt_argument() {
        let args = [
            OsString::from("maolan-burn"),
            OsString::from("warm tape hiss"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.prompt, "warm tape hiss");
        assert_eq!(options.negative_prompt, None);
        assert_eq!(options.backend, BackendChoice::Cpu);
        assert_eq!(options.sampler, SamplerChoice::Dpmpp3mSde);
        assert_eq!(options.cfg_scale, 6.0);
        assert_eq!(options.steps, 250);
        assert_eq!(options.seconds_total, 6);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let args = [
            OsString::from("maolan-burn"),
            OsString::from("  foley footsteps  "),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.prompt, "foley footsteps");
    }

    #[test]
    fn rejects_missing_prompt() {
        let args = [OsString::from("maolan-burn")];
        assert!(parse_options(args).is_err());
    }

    #[test]
    fn parses_backend_flag_after_prompt() {
        let args = [
            OsString::from("maolan-burn"),
            OsString::from("warm tape hiss"),
            OsString::from("--backend"),
            OsString::from("vulkan"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.backend, BackendChoice::Vulkan);
    }

    #[test]
    fn parses_sampler_flag() {
        let args = [
            OsString::from("maolan-burn"),
            OsString::from("--sampler"),
            OsString::from("dpmpp-2m"),
            OsString::from("warm tape hiss"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.sampler, SamplerChoice::Dpmpp2m);
    }

    #[test]
    fn parses_negative_prompt_and_seconds() {
        let args = [
            OsString::from("maolan-burn"),
            OsString::from("--negative-prompt"),
            OsString::from("harsh noise"),
            OsString::from("--cfg-scale"),
            OsString::from("4.5"),
            OsString::from("--steps"),
            OsString::from("300"),
            OsString::from("--seconds-total"),
            OsString::from("8"),
            OsString::from("warm tape hiss"),
        ];
        let options = parse_options(args).expect("options should parse");
        assert_eq!(options.negative_prompt.as_deref(), Some("harsh noise"));
        assert_eq!(options.cfg_scale, 4.5);
        assert_eq!(options.steps, 300);
        assert_eq!(options.seconds_total, 8);
    }

    const _: () = assert!(DEFAULT_MAX_PROMPT_TOKENS == 128);
}
