# maolan-generate

`maolan-generate` is the HeartMuLa generation crate from the Maolan project.
It provides a CLI for prompt-driven music generation and exposes the runtime
pieces the main Maolan application uses for in-process generation and decode.

This directory is a focused package, not the full DAW. The desktop application
and engine live in the repository root and sibling crates.

## What the crate provides

- `maolan-generate`: the main CLI for generating audio from a text prompt or
  lyrics prompt.
- `heartmula_runtime`: runtime helpers used by the CLI and the main app for
  HeartMuLa token generation and HeartCodec decode.
- `heartcodec`: model loading and decode support for the packaged HeartCodec
  path.

The crate currently supports:

- text or lyrics prompts with optional style tags
- CPU or Vulkan backends
- adjustable CFG scale, duration, top-k, temperature, and ODE step count
- decode-only mode from a saved frames JSON
- local model directory overrides or Hugging Face cache resolution

## Model assets

By default the CLI resolves model files through `hf-hub`. The current expected
repositories are:

- `maolandaw/HeartMuLa-happy-new-year-burn`
- `maolandaw/HeartMuLa-RL-oss-3B-20260123`
- `maolandaw/HeartCodec-oss-20260123-burn`

The HeartMuLa repository is expected to provide:

- `heartmula.bpk`
- `tokenizer.json`
- `gen_config.json`

The HeartCodec repository is expected to provide:

- `heartcodec.bpk`

You can bypass Hugging Face cache lookup with `--model-dir <path>` when using a
local Burn export layout.

## CLI usage

Basic generation:

```bash
cargo run -p maolan-generate --release -- "warm pads, slow build, distant vocal"
```

Generation with explicit options:

```bash
cargo run -p maolan-generate --release -- \
  --model happy-new-year \
  --backend vulkan \
  --tags "ambient, cinematic, downtempo" \
  --length 12000 \
  --cfg-scale 1.5 \
  --topk 50 \
  --temperature 1.0 \
  --ode-steps 10 \
  --output output.wav \
  --lyrics "stars drift over the late train home"
```

Decode-only mode from a saved frames JSON:

```bash
cargo run -p maolan-generate --release -- \
  --decode-only \
  --backend cpu \
  --frames-json output.frames.json \
  --output output.wav
```

Run `maolan-generate --help` for the current full option list.

## Development

From the repository root:

```bash
cargo build -p maolan-generate
cargo clippy -p maolan-generate --all-targets
```

The crate is part of the Maolan workspace, so changes here can also affect the
desktop app integration in the root package.

## Repository

- Repository: <https://github.com/maolan/maolan>
- Project site: <https://maolan.github.io>
