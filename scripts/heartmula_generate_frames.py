#!/usr/bin/env python3

import argparse
import json
from pathlib import Path

import torch
from heartlib.pipelines.music_generation import HeartMuLaGenConfig, HeartMuLaGenPipeline
from tokenizers import Tokenizer


def normalize_tags(tags: str) -> str:
    normalized = tags.strip().lower()
    if not normalized.startswith("<tag>"):
        normalized = f"<tag>{normalized}"
    if not normalized.endswith("</tag>"):
        normalized = f"{normalized}</tag>"
    return normalized


def parse_dtype(value: str) -> torch.dtype:
    value = value.lower()
    if value == "f16":
        return torch.float16
    if value == "f32":
        return torch.float32
    raise ValueError(f"unsupported dtype '{value}', expected f16 or f32")


def parse_device(value: str) -> torch.device:
    return torch.device(value)


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate HeartMuLa frames with Python reference model")
    parser.add_argument("--pretrained-root", default="/home/meka/ckpt/heartmula")
    parser.add_argument("--version", default="3B")
    parser.add_argument("--device", default="cpu")
    parser.add_argument("--dtype", default="f32", choices=["f16", "f32"])
    parser.add_argument("--lyrics", required=True)
    parser.add_argument("--tags", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--max-audio-length-ms", type=int, default=2000)
    parser.add_argument("--cfg-scale", type=float, default=6.0)
    parser.add_argument("--topk", type=int, default=50)
    parser.add_argument("--temperature", type=float, default=1.0)
    parser.add_argument("--seed", type=int, default=0)
    args = parser.parse_args()

    torch.manual_seed(args.seed)

    pretrained_root = Path(args.pretrained_root)
    mula_path = pretrained_root / f"HeartMuLa-oss-{args.version}"
    codec_path = pretrained_root / "HeartCodec-oss"
    tokenizer_path = pretrained_root / "tokenizer.json"
    config_path = pretrained_root / "gen_config.json"

    tokenizer = Tokenizer.from_file(str(tokenizer_path))
    config = HeartMuLaGenConfig.from_file(str(config_path))
    device = parse_device(args.device)
    dtype = parse_dtype(args.dtype)

    pipeline = HeartMuLaGenPipeline(
        heartmula_path=str(mula_path),
        heartcodec_path=str(codec_path),
        heartmula_device=device,
        heartcodec_device=device,
        heartmula_dtype=dtype,
        heartcodec_dtype=dtype,
        lazy_load=False,
        muq_mulan=None,
        text_tokenizer=tokenizer,
        config=config,
    )

    lyrics = args.lyrics.strip().lower()
    tags = normalize_tags(args.tags)
    inputs = {"lyrics": lyrics, "tags": tags}
    model_inputs = pipeline.preprocess(inputs, cfg_scale=args.cfg_scale)
    model_outputs = pipeline._forward(
        model_inputs,
        max_audio_length_ms=args.max_audio_length_ms,
        temperature=args.temperature,
        topk=args.topk,
        cfg_scale=args.cfg_scale,
    )
    frames = model_outputs["frames"].to(torch.long).cpu().tolist()

    output = {
        "model": "heartmula",
        "runtime": "python-heartlib",
        "tags": tags,
        "lyrics": lyrics,
        "frames": frames,
        "frame_count": len(frames),
        "sample_rate_hz": 48_000,
    }
    Path(args.output).write_text(json.dumps(output, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
