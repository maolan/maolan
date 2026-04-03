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


def sample_topk_debug(logits: torch.Tensor, topk: int, temperature: float) -> torch.Tensor:
    logits = logits / temperature
    filter_value = -float("inf")
    indices_to_remove = logits < torch.topk(logits, topk)[0][..., -1, None]
    scores_processed = logits.masked_fill(indices_to_remove, filter_value)
    scores_processed = torch.nn.functional.log_softmax(scores_processed, dim=-1)
    return torch.nn.functional.softmax(scores_processed, dim=-1)


def main() -> None:
    parser = argparse.ArgumentParser(description="Dump first-frame deterministic HeartMuLa tensors from Python reference")
    parser.add_argument("--pretrained-root", default="/home/meka/ckpt/heartmula")
    parser.add_argument("--version", default="3B")
    parser.add_argument("--device", default="cpu")
    parser.add_argument("--dtype", default="f32", choices=["f16", "f32"])
    parser.add_argument("--lyrics", required=True)
    parser.add_argument("--tags", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--cfg-scale", type=float, default=6.0)
    parser.add_argument("--topk", type=int, default=50)
    parser.add_argument("--temperature", type=float, default=1.0)
    args = parser.parse_args()

    pretrained_root = Path(args.pretrained_root)
    tok = Tokenizer.from_file(str(pretrained_root / "tokenizer.json"))
    config = HeartMuLaGenConfig.from_file(str(pretrained_root / "gen_config.json"))
    pipeline = HeartMuLaGenPipeline(
        heartmula_path=str(pretrained_root / f"HeartMuLa-oss-{args.version}"),
        heartcodec_path=str(pretrained_root / "HeartCodec-oss"),
        heartmula_device=torch.device(args.device),
        heartcodec_device=torch.device(args.device),
        heartmula_dtype=parse_dtype(args.dtype),
        heartcodec_dtype=parse_dtype(args.dtype),
        lazy_load=True,
        muq_mulan=None,
        text_tokenizer=tok,
        config=config,
    )

    inputs = {"lyrics": args.lyrics.strip().lower(), "tags": normalize_tags(args.tags)}
    model_inputs = pipeline.preprocess(inputs, cfg_scale=args.cfg_scale)
    mula = pipeline.mula
    prompt_tokens = model_inputs["tokens"].to(mula.device)
    prompt_tokens_mask = model_inputs["tokens_mask"].to(mula.device)
    muq_embed = model_inputs["muq_embed"].to(mula.device)
    starts = model_inputs["muq_idx"]
    prompt_pos = model_inputs["pos"].to(mula.device)

    bs_size = 2 if args.cfg_scale != 1.0 else 1
    mula.setup_caches(bs_size)
    with torch.no_grad():
        curr_backbone_mask = mula.backbone_causal_mask[prompt_pos, :]
        uncond_mask = None
        if args.cfg_scale > 1.0 and prompt_tokens.shape[0] > 1:
            actual_b = prompt_tokens.shape[0] // 2
            uncond_mask = torch.cat(
                [
                    torch.zeros(actual_b, dtype=torch.bool, device=prompt_tokens.device),
                    torch.ones(actual_b, dtype=torch.bool, device=prompt_tokens.device),
                ]
            )
        embeds = mula._embed_tokens(prompt_tokens, uncond_mask=uncond_mask)
        masked_embeds = embeds * prompt_tokens_mask.unsqueeze(-1)
        h = masked_embeds.sum(dim=2, dtype=embeds.dtype)
        projected_continuous_segment = None
        if muq_embed is not None:
            projected_continuous_segment = mula.muq_linear(muq_embed)
            if uncond_mask is not None:
                uncond_embed = mula.unconditional_text_embedding(
                    torch.zeros(1, device=prompt_tokens.device, dtype=torch.long)
                )
                mask_expanded = uncond_mask.view(prompt_tokens.shape[0], 1).expand_as(projected_continuous_segment)
                projected_continuous_segment = torch.where(mask_expanded, uncond_embed, projected_continuous_segment)
            batch_indices = torch.arange(h.shape[0], device=h.device)
            h[batch_indices, starts] = projected_continuous_segment
        prefill_input = h.clone()
        layer0 = mula.backbone.layers[0]
        layer0_hidden = layer0.sa_norm(h.clone())
        b, s, _ = layer0_hidden.shape
        q = layer0.attn.q_proj(layer0_hidden).view(
            b, s, layer0.attn.num_heads, layer0.attn.head_dim
        )
        q = layer0.attn.pos_embeddings(q, input_pos=prompt_pos).transpose(1, 2)
        k = layer0.attn.k_proj(layer0_hidden).view(
            b, s, layer0.attn.num_kv_heads, layer0.attn.head_dim
        )
        k = layer0.attn.pos_embeddings(k, input_pos=prompt_pos)
        v = layer0.attn.v_proj(layer0_hidden)
        q_per_kv = layer0.attn.num_heads // layer0.attn.num_kv_heads
        k = k.view(b, s, layer0.attn.num_kv_heads, 1, layer0.attn.head_dim)
        v = v.view(b, s, layer0.attn.num_kv_heads, 1, layer0.attn.head_dim)
        if layer0.attn.num_heads != layer0.attn.num_kv_heads:
            k = k.expand(b, s, layer0.attn.num_kv_heads, q_per_kv, layer0.attn.head_dim)
            v = v.expand(b, s, layer0.attn.num_kv_heads, q_per_kv, layer0.attn.head_dim)
        k = k.reshape(b, s, -1, layer0.attn.head_dim).transpose(1, 2)
        v = v.reshape(b, s, -1, layer0.attn.head_dim).transpose(1, 2)
        h = mula.backbone(h, input_pos=prompt_pos, mask=curr_backbone_mask)
        last_h = h[:, -1, :]
        layer0_cache = mula.backbone.layers[0].attn.kv_cache
        if layer0_cache is None:
            raise RuntimeError("missing backbone layer 0 kv_cache after prefill")
        prefill_k = layer0_cache.k_cache
        prefill_v = layer0_cache.v_cache
        last_layer_cache = mula.backbone.layers[-1].attn.kv_cache
        if last_layer_cache is None:
            raise RuntimeError("missing backbone last layer kv_cache after prefill")
        last_prefill_k = last_layer_cache.k_cache
        last_prefill_v = last_layer_cache.v_cache
        c0_logits = mula.codebook0_head(last_h)
        if args.cfg_scale > 1.0 and c0_logits.shape[0] > 1 and (c0_logits.shape[0] % 2 == 0):
            actual_b = c0_logits.shape[0] // 2
            cond_logits = c0_logits[:actual_b, :]
            uncond_logits = c0_logits[actual_b:, :]
            guided_logits = uncond_logits + (cond_logits - uncond_logits) * args.cfg_scale
        else:
            guided_logits = c0_logits

        argmax_frame = mula.generate_frame(
            prompt_tokens,
            prompt_tokens_mask,
            prompt_pos,
            temperature=1.0,
            topk=1,
            cfg_scale=args.cfg_scale,
            continuous_segments=muq_embed,
            starts=starts,
        )
        argmax_frame = argmax_frame[0].to(torch.int64).cpu().tolist()
        second_history_row = [int(x) for x in argmax_frame] + [0]
        second_tokens = torch.tensor(second_history_row, device=mula.device, dtype=torch.long).view(1, 1, -1)
        second_tokens_mask = torch.ones_like(second_tokens, device=mula.device, dtype=torch.bool)
        second_tokens_mask[..., -1] = False
        second_hidden_input = mula._embed_tokens(second_tokens, uncond_mask=None)
        second_hidden_input = second_hidden_input * second_tokens_mask.unsqueeze(-1)
        second_hidden_input = second_hidden_input.sum(dim=2, dtype=second_hidden_input.dtype)
        if args.cfg_scale > 1.0 and prompt_tokens.shape[0] > 1:
            second_hidden_input = torch.cat([second_hidden_input, second_hidden_input], dim=0)
        second_input_pos = prompt_pos[..., -1:] + 1
        second_layer0 = mula.backbone.layers[0]
        second_layer0_hidden = second_layer0.sa_norm(second_hidden_input.clone())
        sb, ss, _ = second_layer0_hidden.shape
        second_q = second_layer0.attn.q_proj(second_layer0_hidden).view(
            sb, ss, second_layer0.attn.num_heads, second_layer0.attn.head_dim
        )
        second_q = second_layer0.attn.pos_embeddings(second_q, input_pos=second_input_pos).transpose(1, 2)
        second_k = second_layer0.attn.k_proj(second_layer0_hidden).view(
            sb, ss, second_layer0.attn.num_kv_heads, second_layer0.attn.head_dim
        )
        second_k = second_layer0.attn.pos_embeddings(second_k, input_pos=second_input_pos)
        second_v = second_layer0.attn.v_proj(second_layer0_hidden)
        q_per_kv = second_layer0.attn.num_heads // second_layer0.attn.num_kv_heads
        second_k = second_k.view(sb, ss, second_layer0.attn.num_kv_heads, 1, second_layer0.attn.head_dim)
        second_v = second_v.view(sb, ss, second_layer0.attn.num_kv_heads, 1, second_layer0.attn.head_dim)
        if second_layer0.attn.num_heads != second_layer0.attn.num_kv_heads:
            second_k = second_k.expand(sb, ss, second_layer0.attn.num_kv_heads, q_per_kv, second_layer0.attn.head_dim)
            second_v = second_v.expand(sb, ss, second_layer0.attn.num_kv_heads, q_per_kv, second_layer0.attn.head_dim)
        second_k = second_k.reshape(sb, ss, -1, second_layer0.attn.head_dim).transpose(1, 2)
        second_v = second_v.reshape(sb, ss, -1, second_layer0.attn.head_dim).transpose(1, 2)
        second_full_k = prefill_k[:, :, : prompt_pos.shape[1], :]
        second_full_v = prefill_v[:, :, : prompt_pos.shape[1], :]
        second_full_k = torch.cat([second_full_k, second_k], dim=2)
        second_full_v = torch.cat([second_full_v, second_v], dim=2)
        second_scores = torch.matmul(second_q, second_full_k.transpose(2, 3)) * (
            1.0 / (second_layer0.attn.head_dim**0.5)
        )
        second_weights = torch.softmax(second_scores, dim=3)
        second_attn_out = second_layer0.attn.output_proj(
            second_weights.matmul(second_full_v).transpose(1, 2).reshape(sb, ss, -1)
        )
        second_layer0_after_attn = second_hidden_input.clone() + second_attn_out
        second_layer0_mlp_out = second_layer0.mlp(
            second_layer0.mlp_norm(second_layer0_after_attn.clone())
        )
        second_mask = mula.backbone_causal_mask[second_input_pos, :]
        curr = second_hidden_input
        second_layer_outputs_dims = []
        second_layer_outputs = []
        for layer in mula.backbone.layers:
            layer_hidden = layer.sa_norm(curr.clone())
            lb, ls, _ = layer_hidden.shape
            q = layer.attn.q_proj(layer_hidden).view(
                lb, ls, layer.attn.num_heads, layer.attn.head_dim
            )
            q = layer.attn.pos_embeddings(q, input_pos=second_input_pos).transpose(1, 2)
            k = layer.attn.k_proj(layer_hidden).view(
                lb, ls, layer.attn.num_kv_heads, layer.attn.head_dim
            )
            k = layer.attn.pos_embeddings(k, input_pos=second_input_pos)
            v = layer.attn.v_proj(layer_hidden)
            q_per_kv = layer.attn.num_heads // layer.attn.num_kv_heads
            k = k.view(lb, ls, layer.attn.num_kv_heads, 1, layer.attn.head_dim)
            v = v.view(lb, ls, layer.attn.num_kv_heads, 1, layer.attn.head_dim)
            if layer.attn.num_heads != layer.attn.num_kv_heads:
                k = k.expand(lb, ls, layer.attn.num_kv_heads, q_per_kv, layer.attn.head_dim)
                v = v.expand(lb, ls, layer.attn.num_kv_heads, q_per_kv, layer.attn.head_dim)
            k = k.reshape(lb, ls, -1, layer.attn.head_dim).transpose(1, 2)
            v = v.reshape(lb, ls, -1, layer.attn.head_dim).transpose(1, 2)
            layer_cache = layer.attn.kv_cache
            if layer_cache is None:
                raise RuntimeError("missing backbone layer kv_cache during second-step manual walk")
            full_k = torch.cat([layer_cache.k_cache[:, :, : prompt_pos.shape[1], :], k], dim=2)
            full_v = torch.cat([layer_cache.v_cache[:, :, : prompt_pos.shape[1], :], v], dim=2)
            scores = torch.matmul(q, full_k.transpose(2, 3)) * (
                1.0 / (layer.attn.head_dim**0.5)
            )
            weights = torch.softmax(scores, dim=3)
            attn_out = layer.attn.output_proj(
                weights.matmul(full_v).transpose(1, 2).reshape(lb, ls, -1)
            )
            hidden_after_attn = curr + attn_out
            mlp_out = layer.mlp(layer.mlp_norm(hidden_after_attn.clone()))
            curr = hidden_after_attn + mlp_out
            second_layer_outputs_dims.append(list(curr.shape))
            second_layer_outputs.append(curr.to(torch.float32).cpu().reshape(-1).tolist())
        second_hidden = mula.backbone.norm(curr)
        second_last_h = second_hidden[:, -1, :]
        second_c0_logits = mula.codebook0_head(second_last_h)
        if args.cfg_scale > 1.0 and second_c0_logits.shape[0] > 1 and (second_c0_logits.shape[0] % 2 == 0):
            actual_b = second_c0_logits.shape[0] // 2
            cond_logits = second_c0_logits[:actual_b, :]
            uncond_logits = second_c0_logits[actual_b:, :]
            second_guided_logits = uncond_logits + (cond_logits - uncond_logits) * args.cfg_scale
        else:
            second_guided_logits = second_c0_logits
        second_argmax_frame = [int(torch.argmax(second_guided_logits[0]).item())]
        second_guided_decoder_logits = []
        second_guided_decoder_logits_dims = []
        second_decoder_step_inputs_dims = []
        second_decoder_step_inputs = []
        second_decoder_step_hidden_dims = []
        second_decoder_step_hidden = []
        second_decoder_layer0_step2_q_dims = []
        second_decoder_layer0_step2_q = []
        second_decoder_layer0_step2_k_expanded_dims = []
        second_decoder_layer0_step2_k_expanded = []
        second_decoder_layer0_step2_v_expanded_dims = []
        second_decoder_layer0_step2_v_expanded = []
        second_decoder_layer0_step2_full_k_dims = []
        second_decoder_layer0_step2_full_k = []
        second_decoder_layer0_step2_full_v_dims = []
        second_decoder_layer0_step2_full_v = []
        second_c0_sample = torch.tensor([[second_argmax_frame[0]]], device=mula.device, dtype=torch.int64)
        if args.cfg_scale > 1.0 and prompt_tokens.shape[0] > 1 and (prompt_tokens.shape[0] % 2 == 0):
            second_c0_sample = second_c0_sample.repeat(2, 1)
        second_c0_embed = mula._embed_audio(0, second_c0_sample)
        mula.decoder.reset_caches()
        second_curr_h = torch.cat([second_last_h.unsqueeze(1), second_c0_embed], dim=1)
        second_curr_pos = (
            torch.arange(0, second_curr_h.size(1), device=second_curr_h.device)
            .unsqueeze(0)
            .repeat(second_curr_h.size(0), 1)
        )
        second_curr_h = second_curr_h.to(second_hidden_input.dtype)
        for i in range(1, mula.config.audio_num_codebooks):
            second_curr_decoder_mask = mula.decoder_causal_mask[second_curr_pos, :]
            second_projected = mula.projection(second_curr_h)
            second_decoder_step_inputs_dims.append(list(second_projected.shape))
            second_decoder_step_inputs.append(
                second_projected.to(torch.float32).cpu().reshape(-1).tolist()
            )
            if i == 2:
                layer0 = mula.decoder.layers[0]
                layer0_hidden = layer0.sa_norm(second_projected.clone())
                lb, ls, _ = layer0_hidden.shape
                q = layer0.attn.q_proj(layer0_hidden).view(
                    lb, ls, layer0.attn.num_heads, layer0.attn.head_dim
                )
                q = layer0.attn.pos_embeddings(q, input_pos=second_curr_pos).transpose(1, 2)
                k = layer0.attn.k_proj(layer0_hidden).view(
                    lb, ls, layer0.attn.num_kv_heads, layer0.attn.head_dim
                )
                k = layer0.attn.pos_embeddings(k, input_pos=second_curr_pos)
                v = layer0.attn.v_proj(layer0_hidden)
                q_per_kv = layer0.attn.num_heads // layer0.attn.num_kv_heads
                k = k.view(lb, ls, layer0.attn.num_kv_heads, 1, layer0.attn.head_dim)
                v = v.view(lb, ls, layer0.attn.num_kv_heads, 1, layer0.attn.head_dim)
                if layer0.attn.num_heads != layer0.attn.num_kv_heads:
                    k = k.expand(lb, ls, layer0.attn.num_kv_heads, q_per_kv, layer0.attn.head_dim)
                    v = v.expand(lb, ls, layer0.attn.num_kv_heads, q_per_kv, layer0.attn.head_dim)
                k = k.reshape(lb, ls, -1, layer0.attn.head_dim).transpose(1, 2)
                v = v.reshape(lb, ls, -1, layer0.attn.head_dim).transpose(1, 2)
                layer0_cache = layer0.attn.kv_cache
                if layer0_cache is None:
                    raise RuntimeError("missing decoder layer 0 kv_cache during second frame step2")
                full_k = torch.cat([layer0_cache.k_cache[:, :, : second_curr_pos[0, -1].item(), :], k], dim=2)
                full_v = torch.cat([layer0_cache.v_cache[:, :, : second_curr_pos[0, -1].item(), :], v], dim=2)
                second_decoder_layer0_step2_q_dims = list(q.shape)
                second_decoder_layer0_step2_q = q.to(torch.float32).cpu().reshape(-1).tolist()
                second_decoder_layer0_step2_k_expanded_dims = list(k.shape)
                second_decoder_layer0_step2_k_expanded = k.to(torch.float32).cpu().reshape(-1).tolist()
                second_decoder_layer0_step2_v_expanded_dims = list(v.shape)
                second_decoder_layer0_step2_v_expanded = v.to(torch.float32).cpu().reshape(-1).tolist()
                second_decoder_layer0_step2_full_k_dims = list(full_k.shape)
                second_decoder_layer0_step2_full_k = full_k.to(torch.float32).cpu().reshape(-1).tolist()
                second_decoder_layer0_step2_full_v_dims = list(full_v.shape)
                second_decoder_layer0_step2_full_v = full_v.to(torch.float32).cpu().reshape(-1).tolist()
            second_decoder_h = mula.decoder(
                second_projected, input_pos=second_curr_pos, mask=second_curr_decoder_mask
            )
            second_decoder_step_hidden_dims.append(list(second_decoder_h.shape))
            second_decoder_step_hidden.append(
                second_decoder_h.to(torch.float32).cpu().reshape(-1).tolist()
            )
            second_ci_logits = torch.mm(second_decoder_h[:, -1, :], mula.audio_head[i - 1])
            if args.cfg_scale > 1.0 and second_ci_logits.shape[0] > 1 and (second_ci_logits.shape[0] % 2 == 0):
                actual_b = second_ci_logits.shape[0] // 2
                cond_ci = second_ci_logits[:actual_b, :]
                uncond_ci = second_ci_logits[actual_b:, :]
                second_guided_ci = uncond_ci + (cond_ci - uncond_ci) * args.cfg_scale
            else:
                second_guided_ci = second_ci_logits
            second_guided_decoder_logits_dims.append(list(second_guided_ci.shape))
            second_guided_decoder_logits.append(
                second_guided_ci.to(torch.float32).cpu().reshape(-1).tolist()
            )
            second_token = int(torch.argmax(second_guided_ci[0]).item())
            second_argmax_frame.append(second_token)
            second_ci_sample = torch.tensor([[second_token]], device=mula.device, dtype=torch.int64)
            if args.cfg_scale > 1.0 and prompt_tokens.shape[0] > 1 and (prompt_tokens.shape[0] % 2 == 0):
                second_ci_sample = second_ci_sample.repeat(2, 1)
            second_ci_embed = mula._embed_audio(i, second_ci_sample)
            second_curr_h = second_ci_embed
            second_curr_pos = second_curr_pos[:, -1:] + 1

        guided_decoder_logits = []
        guided_decoder_logits_dims = []
        decoder_step_inputs = []
        decoder_step_inputs_dims = []
        decoder_step_hidden = []
        decoder_step_hidden_dims = []
        c0_sample = torch.tensor([[argmax_frame[0]]], device=mula.device, dtype=torch.int64)
        if args.cfg_scale > 1.0 and prompt_tokens.shape[0] > 1 and (prompt_tokens.shape[0] % 2 == 0):
            c0_sample = c0_sample.repeat(2, 1)
        c0_embed = mula._embed_audio(0, c0_sample)
        mula.decoder.reset_caches()
        curr_h = torch.cat([last_h.unsqueeze(1), c0_embed], dim=1)
        curr_pos = (
            torch.arange(0, curr_h.size(1), device=curr_h.device)
            .unsqueeze(0)
            .repeat(curr_h.size(0), 1)
        )
        curr_h = curr_h.to(embeds.dtype)
        for i in range(1, mula.config.audio_num_codebooks):
            curr_decoder_mask = mula.decoder_causal_mask[curr_pos, :]
            projected = mula.projection(curr_h)
            decoder_step_inputs_dims.append(list(projected.shape))
            decoder_step_inputs.append(
                projected.to(torch.float32).cpu().reshape(-1).tolist()
            )
            decoder_h = mula.decoder(projected, input_pos=curr_pos, mask=curr_decoder_mask)
            decoder_step_hidden_dims.append(list(decoder_h.shape))
            decoder_step_hidden.append(
                decoder_h.to(torch.float32).cpu().reshape(-1).tolist()
            )
            ci_logits = torch.mm(decoder_h[:, -1, :], mula.audio_head[i - 1])
            if args.cfg_scale > 1.0 and ci_logits.shape[0] > 1 and (ci_logits.shape[0] % 2 == 0):
                actual_b = ci_logits.shape[0] // 2
                cond_ci = ci_logits[:actual_b, :]
                uncond_ci = ci_logits[actual_b:, :]
                guided_ci = uncond_ci + (cond_ci - uncond_ci) * args.cfg_scale
            else:
                guided_ci = ci_logits
            guided_decoder_logits_dims.append(list(guided_ci.shape))
            guided_decoder_logits.append(
                guided_ci.to(torch.float32).cpu().reshape(-1).tolist()
            )
            ci_sample = torch.tensor([[argmax_frame[i]]], device=mula.device, dtype=torch.int64)
            if args.cfg_scale > 1.0 and prompt_tokens.shape[0] > 1 and (prompt_tokens.shape[0] % 2 == 0):
                ci_sample = ci_sample.repeat(2, 1)
            ci_embed = mula._embed_audio(i, ci_sample)
            curr_h = ci_embed
            curr_pos = curr_pos[:, -1:] + 1

    payload = {
        "tokens": model_inputs["tokens"][0].cpu().tolist(),
        "tokens_mask": model_inputs["tokens_mask"][0].cpu().tolist(),
        "muq_idx": model_inputs["muq_idx"],
        "pos": model_inputs["pos"][0].cpu().tolist(),
        "backbone_prefill_input_dims": list(prefill_input.shape),
        "backbone_prefill_input": prefill_input.to(torch.float32).cpu().reshape(-1).tolist(),
        "backbone_layer0_prefill_hidden_dims": list(layer0_hidden.shape),
        "backbone_layer0_prefill_hidden": layer0_hidden.to(torch.float32).cpu().reshape(-1).tolist(),
        "last_hidden_dims": list(last_h.shape),
        "last_hidden": last_h.to(torch.float32).cpu().reshape(-1).tolist(),
        "backbone_layer0_prefill_q_dims": list(q.shape),
        "backbone_layer0_prefill_q": q.to(torch.float32).cpu().reshape(-1).tolist(),
        "backbone_layer0_prefill_k_expanded_dims": list(k.shape),
        "backbone_layer0_prefill_k_expanded": k.to(torch.float32).cpu().reshape(-1).tolist(),
        "backbone_layer0_prefill_v_expanded_dims": list(v.shape),
        "backbone_layer0_prefill_v_expanded": v.to(torch.float32).cpu().reshape(-1).tolist(),
        "backbone_layer0_prefill_k_dims": list(prefill_k.shape),
        "backbone_layer0_prefill_k": prefill_k.to(torch.float32).cpu().reshape(-1).tolist(),
        "backbone_layer0_prefill_v_dims": list(prefill_v.shape),
        "backbone_layer0_prefill_v": prefill_v.to(torch.float32).cpu().reshape(-1).tolist(),
        "backbone_last_prefill_k_dims": list(last_prefill_k.shape),
        "backbone_last_prefill_k": last_prefill_k.to(torch.float32).cpu().reshape(-1).tolist(),
        "backbone_last_prefill_v_dims": list(last_prefill_v.shape),
        "backbone_last_prefill_v": last_prefill_v.to(torch.float32).cpu().reshape(-1).tolist(),
        "guided_codebook0_logits_dims": list(guided_logits.shape),
        "guided_codebook0_logits": guided_logits.to(torch.float32).cpu().reshape(-1).tolist(),
        "guided_codebook0_probs": sample_topk_debug(guided_logits, args.topk, args.temperature)
        .to(torch.float32)
        .cpu()
        .reshape(-1)
        .tolist(),
        "argmax_first_frame": argmax_frame,
        "second_history_row": second_history_row,
        "second_hidden_input_dims": list(second_hidden_input.shape),
        "second_hidden_input": second_hidden_input.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_layer0_q_dims": list(second_q.shape),
        "second_layer0_q": second_q.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_layer0_k_expanded_dims": list(second_k.shape),
        "second_layer0_k_expanded": second_k.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_layer0_v_expanded_dims": list(second_v.shape),
        "second_layer0_v_expanded": second_v.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_layer0_full_k_dims": list(second_full_k.shape),
        "second_layer0_full_k": second_full_k.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_layer0_full_v_dims": list(second_full_v.shape),
        "second_layer0_full_v": second_full_v.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_layer0_attn_out_dims": list(second_attn_out.shape),
        "second_layer0_attn_out": second_attn_out.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_layer0_mlp_out_dims": list(second_layer0_mlp_out.shape),
        "second_layer0_mlp_out": second_layer0_mlp_out.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_hidden_dims": list(second_last_h.shape),
        "second_hidden": second_last_h.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_layer_outputs_dims": second_layer_outputs_dims,
        "second_layer_outputs": second_layer_outputs,
        "second_guided_codebook0_logits_dims": list(second_guided_logits.shape),
        "second_guided_codebook0_logits": second_guided_logits.to(torch.float32).cpu().reshape(-1).tolist(),
        "second_argmax_frame": second_argmax_frame,
        "second_decoder_step_inputs_dims": second_decoder_step_inputs_dims,
        "second_decoder_step_inputs": second_decoder_step_inputs,
        "second_decoder_step_hidden_dims": second_decoder_step_hidden_dims,
        "second_decoder_step_hidden": second_decoder_step_hidden,
        "second_guided_decoder_logits_dims": second_guided_decoder_logits_dims,
        "second_guided_decoder_logits": second_guided_decoder_logits,
        "second_decoder_layer0_step2_q_dims": second_decoder_layer0_step2_q_dims,
        "second_decoder_layer0_step2_q": second_decoder_layer0_step2_q,
        "second_decoder_layer0_step2_k_expanded_dims": second_decoder_layer0_step2_k_expanded_dims,
        "second_decoder_layer0_step2_k_expanded": second_decoder_layer0_step2_k_expanded,
        "second_decoder_layer0_step2_v_expanded_dims": second_decoder_layer0_step2_v_expanded_dims,
        "second_decoder_layer0_step2_v_expanded": second_decoder_layer0_step2_v_expanded,
        "second_decoder_layer0_step2_full_k_dims": second_decoder_layer0_step2_full_k_dims,
        "second_decoder_layer0_step2_full_k": second_decoder_layer0_step2_full_k,
        "second_decoder_layer0_step2_full_v_dims": second_decoder_layer0_step2_full_v_dims,
        "second_decoder_layer0_step2_full_v": second_decoder_layer0_step2_full_v,
        "guided_decoder_logits_dims": guided_decoder_logits_dims,
        "guided_decoder_logits": guided_decoder_logits,
        "decoder_step_inputs_dims": decoder_step_inputs_dims,
        "decoder_step_inputs": decoder_step_inputs,
        "decoder_step_hidden_dims": decoder_step_hidden_dims,
        "decoder_step_hidden": decoder_step_hidden,
    }
    Path(args.output).write_text(json.dumps(payload, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
