use crate::heartcodec::frames_to_tensor;
use anyhow::{Context, Result, anyhow};
use burn::module::{
    AutodiffModule, ConstantRecord, Content, Devices, Ignored, Module, ModuleDisplay,
    ModuleDisplayDefault, ModuleMapper, ModuleVisitor, Param, ParamId,
};
use burn::nn::{Embedding, EmbeddingConfig, Linear, LinearConfig, LinearLayout};
use burn::prelude::Backend;
use burn::tensor::activation::{silu, softmax};
use burn::tensor::backend::AutodiffBackend;
use burn::tensor::{Bool, DType, Int, Tensor, TensorData};
use burn_store::{BurnpackStore, ModuleSnapshot, ModuleStore};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

macro_rules! eprintln {
    ($($arg:tt)*) => {
        if crate::stderr_logging_enabled() {
            std::eprintln!($($arg)*);
        }
    };
}

const HEARTMULA_PARALLEL_TOKENS: usize = 9;
const HEARTMULA_AUDIO_CODEBOOKS: usize = 8;
const HEARTMULA_HIDDEN_SIZE: usize = 3072;
const HEARTMULA_MUQ_DIM: usize = 512;
const HEARTMULA_BACKBONE_LAYERS: usize = 28;
const HEARTMULA_BACKBONE_HEADS: usize = 24;
const HEARTMULA_BACKBONE_KV_HEADS: usize = 8;
const HEARTMULA_DECODER_LAYERS: usize = 3;
const HEARTMULA_DECODER_HEADS: usize = 8;
const HEARTMULA_DECODER_KV_HEADS: usize = 4;
const HEARTMULA_MLP_DIM: usize = 8192;
const HEARTMULA_NORM_EPSILON: f64 = 1e-5;
const HEARTMULA_ROPE_BASE: f32 = 500_000.0;
const HEARTMULA_ROPE_SCALE_FACTOR: f32 = 32.0;
const HEARTMULA_OLD_CONTEXT_LEN: f32 = 8192.0;
const HEARTMULA_LOW_FREQ_FACTOR: f32 = 1.0;
const HEARTMULA_HIGH_FREQ_FACTOR: f32 = 4.0;
const HEARTCODEC_STAGE_ENV: &str = "MAOLAN_HEARTCODEC_STAGE";
const HEARTCODEC_STAGE_FLOW: &str = "flow";
const HEARTCODEC_STAGE_SCALAR: &str = "scalar";
const HEARTCODEC_STAGE_PLAN_JSON_ENV: &str = "MAOLAN_HEARTCODEC_STAGE_PLAN_JSON";
const HEARTCODEC_STAGE_PLAN_MAGIC: &[u8; 8] = b"MHCPLAN1";
const HEARTCODEC_SEGMENT_DURATION_SECONDS: f32 = 29.76;

type ProgressCallback<'a> = dyn FnMut(&str, f32, &str) + 'a;

#[derive(Debug, Serialize, Deserialize)]
pub struct HeartmulaJsonOutput {
    pub model: String,
    pub runtime: String,
    pub tags: String,
    pub lyrics: String,
    pub frames: Vec<Vec<i64>>,
    pub frame_count: usize,
    pub sample_rate_hz: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HeartmulaFirstFrameDebug {
    pub history: Vec<[i64; HEARTMULA_PARALLEL_TOKENS]>,
    pub backbone_prefill_input_dims: Vec<usize>,
    pub backbone_prefill_input: Vec<f32>,
    pub backbone_layer0_prefill_hidden_dims: Vec<usize>,
    pub backbone_layer0_prefill_hidden: Vec<f32>,
    pub last_hidden_dims: Vec<usize>,
    pub last_hidden: Vec<f32>,
    pub backbone_layer0_prefill_q_dims: Vec<usize>,
    pub backbone_layer0_prefill_q: Vec<f32>,
    pub backbone_layer0_prefill_k_expanded_dims: Vec<usize>,
    pub backbone_layer0_prefill_k_expanded: Vec<f32>,
    pub backbone_layer0_prefill_v_expanded_dims: Vec<usize>,
    pub backbone_layer0_prefill_v_expanded: Vec<f32>,
    pub backbone_layer0_prefill_k_dims: Vec<usize>,
    pub backbone_layer0_prefill_k: Vec<f32>,
    pub backbone_layer0_prefill_v_dims: Vec<usize>,
    pub backbone_layer0_prefill_v: Vec<f32>,
    pub backbone_last_prefill_k_dims: Vec<usize>,
    pub backbone_last_prefill_k: Vec<f32>,
    pub backbone_last_prefill_v_dims: Vec<usize>,
    pub backbone_last_prefill_v: Vec<f32>,
    pub guided_codebook0_logits_dims: Vec<usize>,
    pub guided_codebook0_logits: Vec<f32>,
    pub argmax_first_frame: Vec<i64>,
    pub second_history_row: Vec<i64>,
    pub second_hidden_input_dims: Vec<usize>,
    pub second_hidden_input: Vec<f32>,
    pub second_layer0_q_dims: Vec<usize>,
    pub second_layer0_q: Vec<f32>,
    pub second_layer0_k_expanded_dims: Vec<usize>,
    pub second_layer0_k_expanded: Vec<f32>,
    pub second_layer0_v_expanded_dims: Vec<usize>,
    pub second_layer0_v_expanded: Vec<f32>,
    pub second_layer0_full_k_dims: Vec<usize>,
    pub second_layer0_full_k: Vec<f32>,
    pub second_layer0_full_v_dims: Vec<usize>,
    pub second_layer0_full_v: Vec<f32>,
    pub second_layer0_attn_out_dims: Vec<usize>,
    pub second_layer0_attn_out: Vec<f32>,
    pub second_layer0_mlp_out_dims: Vec<usize>,
    pub second_layer0_mlp_out: Vec<f32>,
    pub second_hidden_dims: Vec<usize>,
    pub second_hidden: Vec<f32>,
    pub second_layer_outputs_dims: Vec<Vec<usize>>,
    pub second_layer_outputs: Vec<Vec<f32>>,
    pub second_guided_codebook0_logits_dims: Vec<usize>,
    pub second_guided_codebook0_logits: Vec<f32>,
    pub second_argmax_frame: Vec<i64>,
    pub second_decoder_step_inputs_dims: Vec<Vec<usize>>,
    pub second_decoder_step_inputs: Vec<Vec<f32>>,
    pub second_decoder_step_hidden_dims: Vec<Vec<usize>>,
    pub second_decoder_step_hidden: Vec<Vec<f32>>,
    pub second_guided_decoder_logits_dims: Vec<Vec<usize>>,
    pub second_guided_decoder_logits: Vec<Vec<f32>>,
    pub second_decoder_layer0_step2_q_dims: Vec<usize>,
    pub second_decoder_layer0_step2_q: Vec<f32>,
    pub second_decoder_layer0_step2_k_expanded_dims: Vec<usize>,
    pub second_decoder_layer0_step2_k_expanded: Vec<f32>,
    pub second_decoder_layer0_step2_v_expanded_dims: Vec<usize>,
    pub second_decoder_layer0_step2_v_expanded: Vec<f32>,
    pub second_decoder_layer0_step2_full_k_dims: Vec<usize>,
    pub second_decoder_layer0_step2_full_k: Vec<f32>,
    pub second_decoder_layer0_step2_full_v_dims: Vec<usize>,
    pub second_decoder_layer0_step2_full_v: Vec<f32>,
    pub guided_decoder_logits_dims: Vec<Vec<usize>>,
    pub guided_decoder_logits: Vec<Vec<f32>>,
    pub decoder_step_inputs_dims: Vec<Vec<usize>>,
    pub decoder_step_inputs: Vec<Vec<f32>>,
    pub decoder_step_hidden_dims: Vec<Vec<usize>>,
    pub decoder_step_hidden: Vec<Vec<f32>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LatentTensorFile {
    dims: [usize; 3],
    data: Vec<f32>,
}

pub struct HeartmulaGenerationConfig<'a> {
    pub text_bos_id: i64,
    pub text_eos_id: i64,
    pub audio_eos_id: i64,
    pub empty_id: i64,
    pub lyrics_ids: &'a [i64],
    pub tags_ids: &'a [i64],
    pub max_audio_frames: usize,
    /// Sampling temperature (higher = more random, lower = more deterministic)
    pub temperature: f32,
    /// Top-k sampling (only consider top k tokens)
    pub topk: usize,
    /// CFG scale for classifier-free guidance (1.0 = no CFG, higher = stronger guidance)
    pub cfg_scale: f32,
    /// Optional progress callback: (phase, progress_0_to_1, operation_description)
    /// phase is either "generator" or "decoder"
    /// Note: This callback is called synchronously on the same thread during generation
    pub progress_callback: Option<Box<ProgressCallback<'a>>>,
}

#[derive(Clone)]
struct HeartmulaTransformerCache<B: Backend> {
    layers: Vec<HeartmulaAttentionCache<B>>,
}

#[derive(Clone)]
struct HeartmulaAttentionCache<B: Backend> {
    key: Option<Tensor<B, 4>>,
    value: Option<Tensor<B, 4>>,
}

#[derive(Clone, Debug)]
struct SplitAudioEmbeddings<B: Backend> {
    table: Option<Tensor<B, 2>>,
    vocab_size: usize,
}

#[derive(Module, Debug)]
pub struct HeartmulaModel<B: Backend> {
    pub text_embeddings: Embedding<B>,
    audio_embeddings: SplitAudioEmbeddings<B>,
    pub unconditional_text_embedding: Embedding<B>,
    pub projection: Linear<B>,
    pub codebook0_head: Linear<B>,
    pub audio_head: Param<Tensor<B, 3>>,
    pub muq_linear: Linear<B>,
    pub backbone: HeartmulaTransformer<B>,
    pub decoder: HeartmulaTransformer<B>,
}

#[derive(Module, Debug)]
pub struct HeartmulaTransformer<B: Backend> {
    pub layers: Vec<HeartmulaTransformerLayer<B>>,
    pub norm: HeartmulaRmsNorm<B>,
}

#[derive(Module, Debug)]
pub struct HeartmulaTransformerLayer<B: Backend> {
    pub attn: HeartmulaAttention<B>,
    pub mlp: HeartmulaMlp<B>,
    pub sa_norm: HeartmulaRmsNorm<B>,
    pub mlp_norm: HeartmulaRmsNorm<B>,
}

#[derive(Module, Debug)]
pub struct HeartmulaAttention<B: Backend> {
    pub q_proj: Linear<B>,
    pub k_proj: Linear<B>,
    pub v_proj: Linear<B>,
    pub output_proj: Linear<B>,
    #[module(skip)]
    meta: Ignored<AttentionMeta>,
}

#[derive(Module, Debug)]
pub struct HeartmulaMlp<B: Backend> {
    pub w1: Linear<B>,
    pub w2: Linear<B>,
    pub w3: Linear<B>,
}

#[derive(Module, Debug)]
pub struct HeartmulaRmsNorm<B: Backend> {
    pub scale: Param<Tensor<B, 1>>,
    pub epsilon: f64,
}

#[derive(Clone, Debug)]
struct AttentionMeta {
    num_heads: usize,
    num_kv_heads: usize,
    head_dim: usize,
}

impl<B: Backend> HeartmulaModel<B> {
    pub fn new(device: &B::Device, text_vocab_size: usize, audio_vocab_size: usize) -> Self {
        Self {
            text_embeddings: EmbeddingConfig::new(text_vocab_size, HEARTMULA_HIDDEN_SIZE)
                .init(device),
            audio_embeddings: SplitAudioEmbeddings::new_placeholder(audio_vocab_size),
            unconditional_text_embedding: EmbeddingConfig::new(1, HEARTMULA_HIDDEN_SIZE)
                .init(device),
            projection: linear_no_bias(device, HEARTMULA_HIDDEN_SIZE, HEARTMULA_HIDDEN_SIZE),
            codebook0_head: linear_no_bias(device, HEARTMULA_HIDDEN_SIZE, audio_vocab_size),
            audio_head: uninitialized_param(
                [
                    HEARTMULA_AUDIO_CODEBOOKS - 1,
                    HEARTMULA_HIDDEN_SIZE,
                    audio_vocab_size,
                ],
                device,
            ),
            muq_linear: linear_with_bias(device, HEARTMULA_MUQ_DIM, HEARTMULA_HIDDEN_SIZE),
            backbone: HeartmulaTransformer::new(
                device,
                HEARTMULA_BACKBONE_LAYERS,
                HEARTMULA_BACKBONE_HEADS,
                HEARTMULA_BACKBONE_KV_HEADS,
            ),
            decoder: HeartmulaTransformer::new(
                device,
                HEARTMULA_DECODER_LAYERS,
                HEARTMULA_DECODER_HEADS,
                HEARTMULA_DECODER_KV_HEADS,
            ),
        }
    }

    pub fn from_burnpack(
        path: &Path,
        device: &B::Device,
        text_vocab_size: usize,
        audio_vocab_size: usize,
    ) -> Result<Self> {
        let mut model = Self::new(device, text_vocab_size, audio_vocab_size);
        let snapshots = BurnpackStore::from_file(path)
            .zero_copy(true)
            .get_all_snapshots()
            .with_context(|| format!("failed to read snapshots from {}", path.display()))?
            .clone();
        let audio_embedding_data = snapshots
            .iter()
            .find_map(|(_, snap)| {
                (snap.full_path() == "audio_embeddings.weight").then(|| snap.to_data().ok())
            })
            .flatten()
            .ok_or_else(|| anyhow!("missing audio_embeddings.weight in HeartMula burnpack"))?
            .convert::<f32>();
        let mut store = BurnpackStore::from_file(path).zero_copy(true);
        model
            .load_from(&mut store)
            .with_context(|| format!("failed to load HeartMula weights from {}", path.display()))?;
        model.audio_embeddings =
            SplitAudioEmbeddings::load_from_data(device, audio_embedding_data, audio_vocab_size)?;
        Ok(model)
    }

    pub fn generate_frames(
        &self,
        device: &B::Device,
        config: &mut HeartmulaGenerationConfig<'_>,
    ) -> Result<Vec<Vec<i64>>> {
        let normalized_tags =
            normalize_text_ids(config.text_bos_id, config.text_eos_id, config.tags_ids);
        let history = build_prompt_history(
            config.text_bos_id,
            config.text_eos_id,
            config.lyrics_ids,
            config.tags_ids,
        );
        let muq_index = normalized_tags.len();
        let mut frames = Vec::new();
        let mut backbone_cache = self.backbone.new_cache();
        let mut last_hidden = self.prefill_backbone(
            device,
            &history,
            Some(muq_index),
            config.cfg_scale > 1.0,
            &mut backbone_cache,
        )?;
        sync_and_cleanup_backend::<B>(device)?;

        // Process in chunks to avoid GPU timeout
        // ~12-13 frames = ~1 second of audio
        const CHUNK_SIZE: usize = 12;
        let total_chunks = config.max_audio_frames.div_ceil(CHUNK_SIZE);

        for chunk_idx in 0..total_chunks {
            let chunk_start = chunk_idx * CHUNK_SIZE;
            let chunk_end = ((chunk_idx + 1) * CHUNK_SIZE).min(config.max_audio_frames);
            let frames_in_chunk = chunk_end - chunk_start;

            eprintln!(
                "  Generating chunk {}/{} (frames {}-{})",
                chunk_idx + 1,
                total_chunks,
                chunk_start,
                chunk_end - 1
            );

            // Report chunk progress (0-99% for generator phase)
            let progress = (chunk_idx as f32 / total_chunks as f32) * 0.99;
            if let Some(ref mut cb) = config.progress_callback {
                cb("generator", progress, "Generating audio tokens");
            }

            for _ in 0..frames_in_chunk {
                if frames.len() >= config.max_audio_frames {
                    break;
                }

                let frame_index = frames.len();
                let next_frame = self.decode_frame_from_last_hidden(
                    device,
                    last_hidden.clone(),
                    config.temperature,
                    config.topk,
                    config.cfg_scale,
                )?;
                if next_frame.iter().any(|token| *token >= config.audio_eos_id) {
                    eprintln!("  EOS token reached at frame {}", frames.len());
                    return Ok(frames);
                }

                frames.push(next_frame);
                eprintln!(
                    "    Finished frame {} / {}",
                    frame_index + 1,
                    config.max_audio_frames
                );
                let next_row = build_audio_history_row(
                    frames.last().expect("frame was just pushed"),
                    config.empty_id,
                );
                let next_hidden = if config.cfg_scale > 1.0 {
                    let hidden = self.embed_single_history_row(device, &next_row);
                    Tensor::cat(vec![hidden.clone(), hidden], 0)
                } else {
                    self.embed_single_history_row(device, &next_row)
                };
                let next_position = (history.len() + frames.len() - 1) as i64;
                last_hidden = self.backbone.forward_incremental(
                    next_hidden,
                    single_position_tensor::<B>(next_position, device),
                    &mut backbone_cache,
                )?;
                sync_and_cleanup_backend::<B>(device)?;
            }

            // Sync device after each chunk to prevent GPU timeout
            // Yield after reclaiming any free transient pages.
            eprintln!("  Syncing device after chunk {}...", chunk_idx + 1);
            sync_and_cleanup_backend::<B>(device)?;
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        Ok(frames)
    }

    pub fn debug_first_frame(
        &self,
        device: &B::Device,
        config: &mut HeartmulaGenerationConfig<'_>,
    ) -> Result<HeartmulaFirstFrameDebug> {
        let normalized_tags =
            normalize_text_ids(config.text_bos_id, config.text_eos_id, config.tags_ids);
        let history = build_prompt_history(
            config.text_bos_id,
            config.text_eos_id,
            config.lyrics_ids,
            config.tags_ids,
        );
        let muq_index = normalized_tags.len();
        let tokens = history_tokens_tensor::<B>(&history, device);
        let tokens_mask = history_mask_tensor::<B>(&history, device);
        let history_hidden_cond = self.embed_history(tokens.clone(), tokens_mask.clone(), false);
        let mut history_hidden_for_debug = if config.cfg_scale > 1.0 {
            let history_hidden_uncond = self.embed_history(tokens, tokens_mask, true);
            Tensor::cat(vec![history_hidden_cond, history_hidden_uncond], 0)
        } else {
            history_hidden_cond
        };
        if Some(muq_index) == Some(muq_index) {
            let muq_zero = Tensor::<B, 2>::zeros([1, HEARTMULA_MUQ_DIM], device);
            let muq_hidden =
                self.muq_linear
                    .forward(muq_zero)
                    .reshape([1, 1, HEARTMULA_HIDDEN_SIZE]);
            history_hidden_for_debug = if config.cfg_scale > 1.0 {
                let uncond_hidden = self
                    .unconditional_text_embedding
                    .forward(Tensor::<B, 2, Int>::zeros([1, 1], device))
                    .reshape([1, 1, HEARTMULA_HIDDEN_SIZE]);
                let replacement = Tensor::cat(vec![muq_hidden, uncond_hidden], 0);
                splice_sequence_token(history_hidden_for_debug, replacement, muq_index)
            } else {
                splice_sequence_token(history_hidden_for_debug, muq_hidden, muq_index)
            };
        }
        let positions = position_tensor::<B>((0..history.len() as i64).collect(), device);
        let layer0 = self
            .backbone
            .layers
            .first()
            .ok_or_else(|| anyhow!("missing backbone layer 0"))?;
        let layer0_hidden = layer0.sa_norm.forward(history_hidden_for_debug.clone());
        let [batch, seq_len, _] = layer0_hidden.dims();
        let mut layer0_q = layer0.attn.q_proj.forward(layer0_hidden.clone()).reshape([
            batch,
            seq_len,
            layer0.attn.meta.0.num_heads,
            layer0.attn.meta.0.head_dim,
        ]);
        let mut layer0_k = layer0.attn.k_proj.forward(layer0_hidden.clone()).reshape([
            batch,
            seq_len,
            layer0.attn.meta.0.num_kv_heads,
            layer0.attn.meta.0.head_dim,
        ]);
        let mut layer0_v = layer0.attn.v_proj.forward(layer0_hidden.clone()).reshape([
            batch,
            seq_len,
            layer0.attn.meta.0.num_kv_heads,
            layer0.attn.meta.0.head_dim,
        ]);
        layer0_q = apply_scaled_rope(layer0_q, &positions);
        layer0_k = apply_scaled_rope(layer0_k, &positions);
        if layer0.attn.meta.0.num_heads != layer0.attn.meta.0.num_kv_heads {
            let repeats = layer0.attn.meta.0.num_heads / layer0.attn.meta.0.num_kv_heads;
            layer0_k = repeat_kv_heads(layer0_k, repeats);
            layer0_v = repeat_kv_heads(layer0_v, repeats);
        }
        let mut backbone_cache = self.backbone.new_cache();
        let last_hidden = self.prefill_backbone(
            device,
            &history,
            Some(muq_index),
            config.cfg_scale > 1.0,
            &mut backbone_cache,
        )?;
        let layer0_cache = backbone_cache
            .layers
            .first()
            .ok_or_else(|| anyhow!("missing backbone layer 0 cache"))?;
        let prefill_k = layer0_cache
            .key
            .clone()
            .ok_or_else(|| anyhow!("missing backbone layer 0 key cache after prefill"))?;
        let prefill_v = layer0_cache
            .value
            .clone()
            .ok_or_else(|| anyhow!("missing backbone layer 0 value cache after prefill"))?;
        let last_layer_cache = backbone_cache
            .layers
            .last()
            .ok_or_else(|| anyhow!("missing backbone last layer cache"))?;
        let last_prefill_k = last_layer_cache
            .key
            .clone()
            .ok_or_else(|| anyhow!("missing backbone last layer key cache after prefill"))?;
        let last_prefill_v = last_layer_cache
            .value
            .clone()
            .ok_or_else(|| anyhow!("missing backbone last layer value cache after prefill"))?;

        let use_cfg = config.cfg_scale > 1.0;
        let codebook0_logits = self.codebook0_head.forward(last_hidden.clone());
        let guided_codebook0_logits = if use_cfg {
            let cond_logits = codebook0_logits
                .clone()
                .slice([0..1, 0..self.audio_vocab_size()]);
            let uncond_logits = codebook0_logits
                .clone()
                .slice([1..2, 0..self.audio_vocab_size()]);
            uncond_logits.clone() + (cond_logits - uncond_logits) * config.cfg_scale
        } else {
            codebook0_logits
        };
        let use_cfg = config.cfg_scale > 1.0;
        let argmax_first_frame = self.decode_frame_from_last_hidden(
            device,
            last_hidden.clone(),
            1.0,
            1,
            config.cfg_scale,
        )?;
        let next_row = build_audio_history_row(&argmax_first_frame, config.empty_id);
        let next_hidden = if use_cfg {
            let hidden = self.embed_single_history_row(device, &next_row);
            Tensor::cat(vec![hidden.clone(), hidden], 0)
        } else {
            self.embed_single_history_row(device, &next_row)
        };
        let next_position = history.len() as i64;
        let second_layer0 = self
            .backbone
            .layers
            .first()
            .ok_or_else(|| anyhow!("missing backbone layer 0"))?;
        let second_layer0_hidden = second_layer0.sa_norm.forward(next_hidden.clone());
        let [second_batch, second_seq_len, _] = second_layer0_hidden.dims();
        let mut second_layer0_q = second_layer0
            .attn
            .q_proj
            .forward(second_layer0_hidden.clone())
            .reshape([
                second_batch,
                second_seq_len,
                second_layer0.attn.meta.0.num_heads,
                second_layer0.attn.meta.0.head_dim,
            ]);
        let mut second_layer0_k_unrepeated = second_layer0
            .attn
            .k_proj
            .forward(second_layer0_hidden.clone())
            .reshape([
                second_batch,
                second_seq_len,
                second_layer0.attn.meta.0.num_kv_heads,
                second_layer0.attn.meta.0.head_dim,
            ]);
        let second_layer0_v_unrepeated = second_layer0
            .attn
            .v_proj
            .forward(second_layer0_hidden.clone())
            .reshape([
                second_batch,
                second_seq_len,
                second_layer0.attn.meta.0.num_kv_heads,
                second_layer0.attn.meta.0.head_dim,
            ]);
        let second_position_tensor = single_position_tensor::<B>(next_position, device);
        second_layer0_q = apply_scaled_rope(second_layer0_q, &second_position_tensor);
        second_layer0_k_unrepeated =
            apply_scaled_rope(second_layer0_k_unrepeated, &second_position_tensor);
        let mut second_layer0_k = second_layer0_k_unrepeated.clone();
        let mut second_layer0_v = second_layer0_v_unrepeated.clone();
        if second_layer0.attn.meta.0.num_heads != second_layer0.attn.meta.0.num_kv_heads {
            let repeats =
                second_layer0.attn.meta.0.num_heads / second_layer0.attn.meta.0.num_kv_heads;
            second_layer0_k = repeat_kv_heads(second_layer0_k, repeats);
            second_layer0_v = repeat_kv_heads(second_layer0_v, repeats);
        }
        let second_layer0_q_swapped = second_layer0_q.clone().swap_dims(1, 2);
        let second_layer0_k_unrepeated = second_layer0_k_unrepeated.swap_dims(1, 2);
        let second_layer0_v_unrepeated = second_layer0_v_unrepeated.swap_dims(1, 2);
        let second_full_k_unrepeated = Tensor::cat(
            vec![prefill_k.clone(), second_layer0_k_unrepeated.clone()],
            2,
        );
        let second_full_v_unrepeated = Tensor::cat(
            vec![prefill_v.clone(), second_layer0_v_unrepeated.clone()],
            2,
        );
        let second_full_k =
            if second_layer0.attn.meta.0.num_heads != second_layer0.attn.meta.0.num_kv_heads {
                let repeats =
                    second_layer0.attn.meta.0.num_heads / second_layer0.attn.meta.0.num_kv_heads;
                repeat_cached_kv_heads(second_full_k_unrepeated.clone(), repeats)
            } else {
                second_full_k_unrepeated.clone()
            };
        let second_full_v =
            if second_layer0.attn.meta.0.num_heads != second_layer0.attn.meta.0.num_kv_heads {
                let repeats =
                    second_layer0.attn.meta.0.num_heads / second_layer0.attn.meta.0.num_kv_heads;
                repeat_cached_kv_heads(second_full_v_unrepeated.clone(), repeats)
            } else {
                second_full_v_unrepeated.clone()
            };
        let second_scores = second_layer0_q_swapped
            .clone()
            .matmul(second_full_k.clone().swap_dims(2, 3))
            .mul_scalar(1.0 / (second_layer0.attn.meta.0.head_dim as f32).sqrt());
        let second_weights = softmax(second_scores, 3);
        let second_attn_out = second_layer0.attn.output_proj.forward(
            second_weights
                .matmul(second_full_v.clone())
                .swap_dims(1, 2)
                .reshape([second_batch, second_seq_len, HEARTMULA_HIDDEN_SIZE]),
        );
        let second_layer0_after_attn = next_hidden.clone() + second_attn_out.clone();
        let second_layer0_mlp_out = second_layer0.mlp.forward(
            second_layer0
                .mlp_norm
                .forward(second_layer0_after_attn.clone()),
        );
        let mut second_layer_outputs_dims = Vec::with_capacity(self.backbone.layers.len());
        let mut second_layer_outputs = Vec::with_capacity(self.backbone.layers.len());
        let mut second_hidden_seq = next_hidden.clone();
        for (layer, layer_cache) in self
            .backbone
            .layers
            .iter()
            .zip(backbone_cache.layers.iter_mut())
        {
            second_hidden_seq = layer.forward_incremental(
                second_hidden_seq,
                second_position_tensor.clone(),
                layer_cache,
            )?;
            second_layer_outputs_dims.push(second_hidden_seq.dims().to_vec());
            second_layer_outputs.push(tensor_to_f32_vec(second_hidden_seq.clone())?);
        }
        let second_hidden = take_last_token(self.backbone.norm.forward(second_hidden_seq));
        let second_codebook0_logits = self.codebook0_head.forward(second_hidden.clone());
        let second_guided_codebook0_logits = if use_cfg {
            let cond_logits = second_codebook0_logits
                .clone()
                .slice([0..1, 0..self.audio_vocab_size()]);
            let uncond_logits = second_codebook0_logits
                .clone()
                .slice([1..2, 0..self.audio_vocab_size()]);
            uncond_logits.clone() + (cond_logits - uncond_logits) * config.cfg_scale
        } else {
            second_codebook0_logits
        };
        let second_argmax_frame = self.decode_frame_from_last_hidden(
            device,
            second_hidden.clone(),
            1.0,
            1,
            config.cfg_scale,
        )?;
        let mut second_guided_decoder_logits_dims =
            Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut second_guided_decoder_logits = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut second_decoder_step_inputs_dims = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut second_decoder_step_inputs = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut second_decoder_step_hidden_dims = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut second_decoder_step_hidden = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut second_decoder_cache = self.decoder.new_cache();
        let second_c0_token = *second_argmax_frame
            .first()
            .ok_or_else(|| anyhow!("second argmax frame was empty"))?;
        let second_c0_embed = self.embed_audio_token(device, 0, second_c0_token);
        let second_c0_embed = if use_cfg {
            Tensor::cat(vec![second_c0_embed.clone(), second_c0_embed], 0)
        } else {
            second_c0_embed
        };
        let second_decoder_input = Tensor::cat(
            vec![
                second_hidden.clone().unsqueeze_dim(1),
                second_c0_embed.clone(),
            ],
            1,
        );
        let second_decoder_input = self.projection.forward(second_decoder_input);
        let second_first_decoder_h = self.decoder.forward_prefill(
            second_decoder_input.clone(),
            position_tensor::<B>(vec![0, 1], device),
            &mut second_decoder_cache,
        )?;
        let mut second_decoder_layer0_step2_q_dims = Vec::new();
        let mut second_decoder_layer0_step2_q = Vec::new();
        let mut second_decoder_layer0_step2_k_expanded_dims = Vec::new();
        let mut second_decoder_layer0_step2_k_expanded = Vec::new();
        let mut second_decoder_layer0_step2_v_expanded_dims = Vec::new();
        let mut second_decoder_layer0_step2_v_expanded = Vec::new();
        let mut second_decoder_layer0_step2_full_k_dims = Vec::new();
        let mut second_decoder_layer0_step2_full_k = Vec::new();
        let mut second_decoder_layer0_step2_full_v_dims = Vec::new();
        let mut second_decoder_layer0_step2_full_v = Vec::new();
        let mut second_current_embed: Option<Tensor<B, 3>> = None;
        let mut second_next_decoder_pos = 2_i64;
        for codebook in 1..HEARTMULA_AUDIO_CODEBOOKS {
            if codebook == 2 {
                let embed = second_current_embed.clone().ok_or_else(|| {
                    anyhow!("missing second decoder embed for codebook {}", codebook)
                })?;
                let second_decoder_input = self.projection.forward(embed.clone());
                let layer0 = self
                    .decoder
                    .layers
                    .first()
                    .ok_or_else(|| anyhow!("missing decoder layer 0"))?;
                let layer0_hidden = layer0.sa_norm.forward(second_decoder_input.clone());
                let [batch, seq_len, _] = layer0_hidden.dims();
                let mut q = layer0.attn.q_proj.forward(layer0_hidden.clone()).reshape([
                    batch,
                    seq_len,
                    layer0.attn.meta.0.num_heads,
                    layer0.attn.meta.0.head_dim,
                ]);
                let mut k_unrepeated = layer0.attn.k_proj.forward(layer0_hidden.clone()).reshape([
                    batch,
                    seq_len,
                    layer0.attn.meta.0.num_kv_heads,
                    layer0.attn.meta.0.head_dim,
                ]);
                let v_unrepeated = layer0.attn.v_proj.forward(layer0_hidden.clone()).reshape([
                    batch,
                    seq_len,
                    layer0.attn.meta.0.num_kv_heads,
                    layer0.attn.meta.0.head_dim,
                ]);
                let pos = single_position_tensor::<B>(second_next_decoder_pos, device);
                q = apply_scaled_rope(q, &pos);
                k_unrepeated = apply_scaled_rope(k_unrepeated, &pos);
                let mut k = k_unrepeated.clone();
                let mut v = v_unrepeated.clone();
                if layer0.attn.meta.0.num_heads != layer0.attn.meta.0.num_kv_heads {
                    let repeats = layer0.attn.meta.0.num_heads / layer0.attn.meta.0.num_kv_heads;
                    k = repeat_kv_heads(k, repeats);
                    v = repeat_kv_heads(v, repeats);
                }
                let q_swapped = q.clone().swap_dims(1, 2);
                let k_unrepeated = k_unrepeated.swap_dims(1, 2);
                let v_unrepeated = v_unrepeated.swap_dims(1, 2);
                let layer0_cache = second_decoder_cache
                    .layers
                    .first()
                    .ok_or_else(|| anyhow!("missing decoder layer 0 cache"))?;
                let prev_k = layer0_cache
                    .key
                    .clone()
                    .ok_or_else(|| anyhow!("missing decoder layer 0 key cache"))?;
                let prev_v = layer0_cache
                    .value
                    .clone()
                    .ok_or_else(|| anyhow!("missing decoder layer 0 value cache"))?;
                let full_k_unrepeated = Tensor::cat(vec![prev_k, k_unrepeated.clone()], 2);
                let full_v_unrepeated = Tensor::cat(vec![prev_v, v_unrepeated.clone()], 2);
                let full_k = if layer0.attn.meta.0.num_heads != layer0.attn.meta.0.num_kv_heads {
                    let repeats = layer0.attn.meta.0.num_heads / layer0.attn.meta.0.num_kv_heads;
                    repeat_cached_kv_heads(full_k_unrepeated, repeats)
                } else {
                    full_k_unrepeated
                };
                let full_v = if layer0.attn.meta.0.num_heads != layer0.attn.meta.0.num_kv_heads {
                    let repeats = layer0.attn.meta.0.num_heads / layer0.attn.meta.0.num_kv_heads;
                    repeat_cached_kv_heads(full_v_unrepeated, repeats)
                } else {
                    full_v_unrepeated
                };
                second_decoder_layer0_step2_q_dims = q_swapped.dims().to_vec();
                second_decoder_layer0_step2_q = tensor_to_f32_vec(q_swapped)?;
                second_decoder_layer0_step2_k_expanded_dims = k.dims().to_vec();
                second_decoder_layer0_step2_k_expanded = tensor_to_f32_vec(k)?;
                second_decoder_layer0_step2_v_expanded_dims = v.dims().to_vec();
                second_decoder_layer0_step2_v_expanded = tensor_to_f32_vec(v)?;
                second_decoder_layer0_step2_full_k_dims = full_k.dims().to_vec();
                second_decoder_layer0_step2_full_k = tensor_to_f32_vec(full_k)?;
                second_decoder_layer0_step2_full_v_dims = full_v.dims().to_vec();
                second_decoder_layer0_step2_full_v = tensor_to_f32_vec(full_v)?;
            }
            let head = self
                .audio_head
                .val()
                .slice([
                    codebook - 1..codebook,
                    0..HEARTMULA_HIDDEN_SIZE,
                    0..self.audio_head.dims()[2],
                ])
                .reshape([HEARTMULA_HIDDEN_SIZE, self.audio_head.dims()[2]]);
            let logits = if codebook == 1 {
                second_decoder_step_inputs_dims.push(second_decoder_input.dims().to_vec());
                second_decoder_step_inputs.push(tensor_to_f32_vec(second_decoder_input.clone())?);
                second_decoder_step_hidden_dims.push(second_first_decoder_h.dims().to_vec());
                second_decoder_step_hidden.push(tensor_to_f32_vec(second_first_decoder_h.clone())?);
                second_first_decoder_h.clone().matmul(head.clone())
            } else {
                let embed = second_current_embed.clone().ok_or_else(|| {
                    anyhow!("missing second decoder embed for codebook {}", codebook)
                })?;
                let second_decoder_input = self.projection.forward(embed);
                second_decoder_step_inputs_dims.push(second_decoder_input.dims().to_vec());
                second_decoder_step_inputs.push(tensor_to_f32_vec(second_decoder_input.clone())?);
                let second_last_decoder_h = self.decoder.forward_incremental(
                    second_decoder_input,
                    single_position_tensor::<B>(second_next_decoder_pos, device),
                    &mut second_decoder_cache,
                )?;
                second_decoder_step_hidden_dims.push(second_last_decoder_h.dims().to_vec());
                second_decoder_step_hidden.push(tensor_to_f32_vec(second_last_decoder_h.clone())?);
                second_next_decoder_pos += 1;
                second_last_decoder_h.matmul(head.clone())
            };
            let guided_logits = if use_cfg {
                let cond_logits = logits.clone().slice([0..1, 0..self.audio_vocab_size()]);
                let uncond_logits = logits.slice([1..2, 0..self.audio_vocab_size()]);
                uncond_logits.clone() + (cond_logits - uncond_logits) * config.cfg_scale
            } else {
                logits
            };
            second_guided_decoder_logits_dims.push(guided_logits.dims().to_vec());
            second_guided_decoder_logits.push(tensor_to_f32_vec(guided_logits.clone())?);
            let token = *second_argmax_frame
                .get(codebook)
                .ok_or_else(|| anyhow!("missing second argmax token for codebook {}", codebook))?;
            second_current_embed = Some(self.embed_audio_token(device, codebook, token));
            if use_cfg {
                let embed = second_current_embed
                    .clone()
                    .ok_or_else(|| anyhow!("missing second decoder embed after sampling"))?;
                second_current_embed = Some(Tensor::cat(vec![embed.clone(), embed], 0));
            }
        }
        let mut guided_decoder_logits_dims = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut guided_decoder_logits = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut decoder_step_inputs_dims = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut decoder_step_inputs = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut decoder_step_hidden_dims = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);
        let mut decoder_step_hidden = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS - 1);

        let mut decoder_cache = self.decoder.new_cache();
        let c0_token = *argmax_first_frame
            .first()
            .ok_or_else(|| anyhow!("argmax first frame was empty"))?;
        let c0_embed = self.embed_audio_token(device, 0, c0_token);
        let c0_embed = if use_cfg {
            Tensor::cat(vec![c0_embed.clone(), c0_embed], 0)
        } else {
            c0_embed
        };
        let decoder_input = Tensor::cat(
            vec![last_hidden.clone().unsqueeze_dim(1), c0_embed.clone()],
            1,
        );
        let decoder_input = self.projection.forward(decoder_input);
        let first_decoder_h = self.decoder.forward_prefill(
            decoder_input.clone(),
            position_tensor::<B>(vec![0, 1], device),
            &mut decoder_cache,
        )?;
        let mut current_embed: Option<Tensor<B, 3>> = None;
        let mut next_decoder_pos = 2_i64;
        for codebook in 1..HEARTMULA_AUDIO_CODEBOOKS {
            let head = self
                .audio_head
                .val()
                .slice([
                    codebook - 1..codebook,
                    0..HEARTMULA_HIDDEN_SIZE,
                    0..self.audio_head.dims()[2],
                ])
                .reshape([HEARTMULA_HIDDEN_SIZE, self.audio_head.dims()[2]]);
            let logits = if codebook == 1 {
                decoder_step_inputs_dims.push(decoder_input.dims().to_vec());
                decoder_step_inputs.push(tensor_to_f32_vec(decoder_input.clone())?);
                decoder_step_hidden_dims.push(first_decoder_h.dims().to_vec());
                decoder_step_hidden.push(tensor_to_f32_vec(first_decoder_h.clone())?);
                first_decoder_h.clone().matmul(head.clone())
            } else {
                let embed = current_embed.clone().ok_or_else(|| {
                    anyhow!("missing debug decoder embed for codebook {}", codebook)
                })?;
                let decoder_input = self.projection.forward(embed);
                decoder_step_inputs_dims.push(decoder_input.dims().to_vec());
                decoder_step_inputs.push(tensor_to_f32_vec(decoder_input.clone())?);
                let last_decoder_h = self.decoder.forward_incremental(
                    decoder_input,
                    single_position_tensor::<B>(next_decoder_pos, device),
                    &mut decoder_cache,
                )?;
                decoder_step_hidden_dims.push(last_decoder_h.dims().to_vec());
                decoder_step_hidden.push(tensor_to_f32_vec(last_decoder_h.clone())?);
                next_decoder_pos += 1;
                last_decoder_h.matmul(head.clone())
            };
            let guided_logits = if use_cfg {
                let cond_logits = logits.clone().slice([0..1, 0..self.audio_vocab_size()]);
                let uncond_logits = logits.slice([1..2, 0..self.audio_vocab_size()]);
                uncond_logits.clone() + (cond_logits - uncond_logits) * config.cfg_scale
            } else {
                logits
            };
            guided_decoder_logits_dims.push(guided_logits.dims().to_vec());
            guided_decoder_logits.push(tensor_to_f32_vec(guided_logits.clone())?);
            let token = *argmax_first_frame
                .get(codebook)
                .ok_or_else(|| anyhow!("missing argmax token for codebook {}", codebook))?;
            current_embed = Some(self.embed_audio_token(device, codebook, token));
            if use_cfg {
                let embed = current_embed
                    .clone()
                    .ok_or_else(|| anyhow!("missing debug decoder embed after sampling"))?;
                current_embed = Some(Tensor::cat(vec![embed.clone(), embed], 0));
            }
        }

        Ok(HeartmulaFirstFrameDebug {
            history,
            backbone_prefill_input_dims: history_hidden_for_debug.dims().to_vec(),
            backbone_prefill_input: tensor_to_f32_vec(history_hidden_for_debug)?,
            backbone_layer0_prefill_hidden_dims: layer0_hidden.dims().to_vec(),
            backbone_layer0_prefill_hidden: tensor_to_f32_vec(layer0_hidden)?,
            last_hidden_dims: last_hidden.dims().to_vec(),
            last_hidden: tensor_to_f32_vec(last_hidden)?,
            backbone_layer0_prefill_q_dims: layer0_q.dims().to_vec(),
            backbone_layer0_prefill_q: tensor_to_f32_vec(layer0_q)?,
            backbone_layer0_prefill_k_expanded_dims: layer0_k.dims().to_vec(),
            backbone_layer0_prefill_k_expanded: tensor_to_f32_vec(layer0_k.clone())?,
            backbone_layer0_prefill_v_expanded_dims: layer0_v.dims().to_vec(),
            backbone_layer0_prefill_v_expanded: tensor_to_f32_vec(layer0_v.clone())?,
            backbone_layer0_prefill_k_dims: prefill_k.dims().to_vec(),
            backbone_layer0_prefill_k: tensor_to_f32_vec(prefill_k)?,
            backbone_layer0_prefill_v_dims: prefill_v.dims().to_vec(),
            backbone_layer0_prefill_v: tensor_to_f32_vec(prefill_v)?,
            backbone_last_prefill_k_dims: last_prefill_k.dims().to_vec(),
            backbone_last_prefill_k: tensor_to_f32_vec(last_prefill_k)?,
            backbone_last_prefill_v_dims: last_prefill_v.dims().to_vec(),
            backbone_last_prefill_v: tensor_to_f32_vec(last_prefill_v)?,
            guided_codebook0_logits_dims: guided_codebook0_logits.dims().to_vec(),
            guided_codebook0_logits: tensor_to_f32_vec(guided_codebook0_logits)?,
            argmax_first_frame,
            second_history_row: next_row.to_vec(),
            second_hidden_input_dims: next_hidden.dims().to_vec(),
            second_hidden_input: tensor_to_f32_vec(next_hidden.clone())?,
            second_layer0_q_dims: second_layer0_q_swapped.dims().to_vec(),
            second_layer0_q: tensor_to_f32_vec(second_layer0_q_swapped)?,
            second_layer0_k_expanded_dims: second_layer0_k.dims().to_vec(),
            second_layer0_k_expanded: tensor_to_f32_vec(second_layer0_k)?,
            second_layer0_v_expanded_dims: second_layer0_v.dims().to_vec(),
            second_layer0_v_expanded: tensor_to_f32_vec(second_layer0_v)?,
            second_layer0_full_k_dims: second_full_k.dims().to_vec(),
            second_layer0_full_k: tensor_to_f32_vec(second_full_k)?,
            second_layer0_full_v_dims: second_full_v.dims().to_vec(),
            second_layer0_full_v: tensor_to_f32_vec(second_full_v)?,
            second_layer0_attn_out_dims: second_attn_out.dims().to_vec(),
            second_layer0_attn_out: tensor_to_f32_vec(second_attn_out)?,
            second_layer0_mlp_out_dims: second_layer0_mlp_out.dims().to_vec(),
            second_layer0_mlp_out: tensor_to_f32_vec(second_layer0_mlp_out)?,
            second_hidden_dims: second_hidden.dims().to_vec(),
            second_hidden: tensor_to_f32_vec(second_hidden)?,
            second_layer_outputs_dims,
            second_layer_outputs,
            second_guided_codebook0_logits_dims: second_guided_codebook0_logits.dims().to_vec(),
            second_guided_codebook0_logits: tensor_to_f32_vec(second_guided_codebook0_logits)?,
            second_argmax_frame,
            second_decoder_step_inputs_dims,
            second_decoder_step_inputs,
            second_decoder_step_hidden_dims,
            second_decoder_step_hidden,
            second_guided_decoder_logits_dims,
            second_guided_decoder_logits,
            second_decoder_layer0_step2_q_dims,
            second_decoder_layer0_step2_q,
            second_decoder_layer0_step2_k_expanded_dims,
            second_decoder_layer0_step2_k_expanded,
            second_decoder_layer0_step2_v_expanded_dims,
            second_decoder_layer0_step2_v_expanded,
            second_decoder_layer0_step2_full_k_dims,
            second_decoder_layer0_step2_full_k,
            second_decoder_layer0_step2_full_v_dims,
            second_decoder_layer0_step2_full_v,
            guided_decoder_logits_dims,
            guided_decoder_logits,
            decoder_step_inputs_dims,
            decoder_step_inputs,
            decoder_step_hidden_dims,
            decoder_step_hidden,
        })
    }

    fn prefill_backbone(
        &self,
        device: &B::Device,
        history: &[[i64; HEARTMULA_PARALLEL_TOKENS]],
        muq_insert_index: Option<usize>,
        use_cfg: bool,
        cache: &mut HeartmulaTransformerCache<B>,
    ) -> Result<Tensor<B, 2>> {
        let tokens = history_tokens_tensor::<B>(history, device);
        let tokens_mask = history_mask_tensor::<B>(history, device);
        let history_hidden_cond = self.embed_history(tokens.clone(), tokens_mask.clone(), false);
        let mut history_hidden = if use_cfg {
            let history_hidden_uncond = self.embed_history(tokens, tokens_mask, true);
            Tensor::cat(vec![history_hidden_cond, history_hidden_uncond], 0)
        } else {
            history_hidden_cond
        };

        if let Some(index) = muq_insert_index {
            let muq_zero = Tensor::<B, 2>::zeros([1, HEARTMULA_MUQ_DIM], device);
            let muq_hidden =
                self.muq_linear
                    .forward(muq_zero)
                    .reshape([1, 1, HEARTMULA_HIDDEN_SIZE]);
            history_hidden = if use_cfg {
                let uncond_hidden = self
                    .unconditional_text_embedding
                    .forward(Tensor::<B, 2, Int>::zeros([1, 1], device))
                    .reshape([1, 1, HEARTMULA_HIDDEN_SIZE]);
                let replacement = Tensor::cat(vec![muq_hidden, uncond_hidden], 0);
                splice_sequence_token(history_hidden, replacement, index)
            } else {
                splice_sequence_token(history_hidden, muq_hidden, index)
            };
        }

        let positions = position_tensor::<B>((0..history.len() as i64).collect(), device);
        self.backbone
            .forward_prefill(history_hidden, positions, cache)
    }

    fn decode_frame_from_last_hidden(
        &self,
        device: &B::Device,
        last_hidden: Tensor<B, 2>,
        temperature: f32,
        topk: usize,
        cfg_scale: f32,
    ) -> Result<Vec<i64>> {
        let use_cfg = cfg_scale > 1.0;

        let codebook0_logits = self.codebook0_head.forward(last_hidden.clone());

        let cond_codebook0_logits = if use_cfg {
            codebook0_logits
                .clone()
                .slice([0..1, 0..self.audio_vocab_size()])
        } else {
            codebook0_logits.clone()
        };
        let uncond_codebook0_logits = if use_cfg {
            codebook0_logits
                .clone()
                .slice([1..2, 0..self.audio_vocab_size()])
        } else {
            codebook0_logits.clone()
        };
        let codebook0_logits = if use_cfg {
            uncond_codebook0_logits.clone()
                + (cond_codebook0_logits - uncond_codebook0_logits) * cfg_scale
        } else {
            codebook0_logits
        };

        let mut frame = Vec::with_capacity(HEARTMULA_AUDIO_CODEBOOKS);
        let first_token = sample_token(&codebook0_logits, temperature, topk)?;
        frame.push(first_token);

        // Initialize decoder with concatenated [projected_last_hidden, c0_embed]
        // This matches Python: curr_h = torch.cat([last_h.unsqueeze(1), c0_embed], dim=1)
        let mut decoder_cache = self.decoder.new_cache();
        let c0_embed = self.embed_audio_token(device, 0, first_token);
        let c0_embed = if use_cfg {
            Tensor::cat(vec![c0_embed.clone(), c0_embed], 0)
        } else {
            c0_embed
        };
        let decoder_input = Tensor::cat(
            vec![last_hidden.clone().unsqueeze_dim(1), c0_embed.clone()],
            1,
        );
        let decoder_input = self.projection.forward(decoder_input);
        let first_decoder_h = self.decoder.forward_prefill(
            decoder_input,
            position_tensor::<B>(vec![0, 1], device),
            &mut decoder_cache,
        )?;
        let mut current_embed: Option<Tensor<B, 3>> = None;
        let mut next_decoder_pos = 2_i64;
        for codebook in 1..HEARTMULA_AUDIO_CODEBOOKS {
            let head = self
                .audio_head
                .val()
                .slice([
                    codebook - 1..codebook,
                    0..HEARTMULA_HIDDEN_SIZE,
                    0..self.audio_head.dims()[2],
                ])
                .reshape([HEARTMULA_HIDDEN_SIZE, self.audio_head.dims()[2]]);
            let logits = if codebook == 1 {
                first_decoder_h.clone().matmul(head.clone())
            } else {
                let embed = current_embed
                    .clone()
                    .ok_or_else(|| anyhow!("missing decoder embed for codebook {}", codebook))?;
                let decoder_input = self.projection.forward(embed);
                let last_decoder_h = self.decoder.forward_incremental(
                    decoder_input,
                    single_position_tensor::<B>(next_decoder_pos, device),
                    &mut decoder_cache,
                )?;
                next_decoder_pos += 1;
                last_decoder_h.matmul(head.clone())
            };
            let logits = if use_cfg {
                let cond_logits = logits.clone().slice([0..1, 0..self.audio_vocab_size()]);
                let uncond_logits = logits.slice([1..2, 0..self.audio_vocab_size()]);
                uncond_logits.clone() + (cond_logits - uncond_logits) * cfg_scale
            } else {
                logits
            };

            let token = sample_token(&logits, temperature, topk)?;
            frame.push(token);
            current_embed = Some(self.embed_audio_token(device, codebook, token));
            if use_cfg {
                let embed = current_embed
                    .clone()
                    .ok_or_else(|| anyhow!("missing decoder embed after sampling"))?;
                current_embed = Some(Tensor::cat(vec![embed.clone(), embed], 0));
            }
        }

        Ok(frame)
    }

    fn embed_history(
        &self,
        tokens: Tensor<B, 3, Int>,
        tokens_mask: Tensor<B, 3, Bool>,
        use_unconditional_text: bool,
    ) -> Tensor<B, 3> {
        let [batch, seq_len, _] = tokens.dims();
        let text_ids = tokens
            .clone()
            .slice([
                0..batch,
                0..seq_len,
                HEARTMULA_AUDIO_CODEBOOKS..HEARTMULA_PARALLEL_TOKENS,
            ])
            .reshape([batch, seq_len]);
        let audio_ids = tokens
            .slice([0..batch, 0..seq_len, 0..HEARTMULA_AUDIO_CODEBOOKS])
            .reshape([batch, seq_len * HEARTMULA_AUDIO_CODEBOOKS]);
        let offsets = (0..HEARTMULA_AUDIO_CODEBOOKS)
            .map(|index| (index * self.audio_vocab_size()) as i64)
            .collect::<Vec<_>>();
        let offset_tensor =
            Tensor::<B, 1, Int>::from_data(offsets.as_slice(), &tokens_mask.device()).reshape([
                1,
                1,
                HEARTMULA_AUDIO_CODEBOOKS,
            ]);
        let shifted_audio_ids = audio_ids
            .reshape([batch, seq_len, HEARTMULA_AUDIO_CODEBOOKS])
            .add(offset_tensor);

        let text_embeds = if use_unconditional_text {
            self.unconditional_text_embedding
                .forward(Tensor::<B, 2, Int>::zeros(
                    [batch, seq_len],
                    &tokens_mask.device(),
                ))
                .unsqueeze_dim(2)
        } else {
            self.text_embeddings.forward(text_ids).unsqueeze_dim(2)
        };
        let audio_embeds = self.audio_embeddings.forward(shifted_audio_ids);
        let text_embeds = if text_embeds.dims()[0] == audio_embeds.dims()[0] {
            text_embeds
        } else {
            text_embeds.repeat_dim(0, audio_embeds.dims()[0])
        };
        let embeds = Tensor::cat(vec![audio_embeds, text_embeds], 2);
        let mask = tokens_mask
            .reshape([batch, seq_len, HEARTMULA_PARALLEL_TOKENS, 1])
            .repeat_dim(3, HEARTMULA_HIDDEN_SIZE)
            .float();

        (embeds * mask)
            .sum_dim(2)
            .reshape([batch, seq_len, HEARTMULA_HIDDEN_SIZE])
    }

    fn embed_audio_token(&self, device: &B::Device, codebook: usize, token: i64) -> Tensor<B, 3> {
        let offset_token = token + (codebook * self.audio_vocab_size()) as i64;
        self.audio_embeddings
            .embed_offset_token(device, offset_token)
    }

    fn audio_vocab_size(&self) -> usize {
        self.audio_head.dims()[2]
    }

    fn embed_single_history_row(
        &self,
        device: &B::Device,
        row: &[i64; HEARTMULA_PARALLEL_TOKENS],
    ) -> Tensor<B, 3> {
        let tokens = Tensor::<B, 3, Int>::from_data(
            TensorData::new(row.to_vec(), [1, 1, HEARTMULA_PARALLEL_TOKENS]),
            device,
        );
        let mask = Tensor::<B, 3, Bool>::from_data(
            TensorData::new(
                vec![true, true, true, true, true, true, true, true, false],
                [1, 1, HEARTMULA_PARALLEL_TOKENS],
            ),
            device,
        );
        self.embed_history(tokens, mask, false)
    }
}

fn sync_and_cleanup_backend<B: Backend>(device: &B::Device) -> Result<()> {
    B::sync(device)?;
    B::memory_cleanup(device);
    Ok(())
}

impl<B: Backend> SplitAudioEmbeddings<B> {
    fn new_placeholder(vocab_size: usize) -> Self {
        Self {
            table: None,
            vocab_size,
        }
    }

    fn load_from_data(device: &B::Device, data: TensorData, vocab_size: usize) -> Result<Self> {
        let shape = data.shape.clone();
        let expected_rows = vocab_size * HEARTMULA_AUDIO_CODEBOOKS;
        if shape.as_slice() != [expected_rows, HEARTMULA_HIDDEN_SIZE] {
            anyhow::bail!(
                "unexpected audio_embeddings.weight shape {:?}, expected [{}, {}]",
                shape,
                expected_rows,
                HEARTMULA_HIDDEN_SIZE
            );
        }
        Ok(Self {
            table: Some(Tensor::<B, 2>::from_data(data, device)),
            vocab_size,
        })
    }

    fn forward(&self, offset_audio_ids: Tensor<B, 3, Int>) -> Tensor<B, 4> {
        let [batch, seq_len, codebooks] = offset_audio_ids.dims();
        debug_assert_eq!(codebooks, HEARTMULA_AUDIO_CODEBOOKS);
        let table = self
            .table
            .as_ref()
            .expect("audio embeddings must be loaded before use")
            .clone();
        let ids = offset_audio_ids.reshape([batch * seq_len * codebooks]);
        table
            .select(0, ids)
            .reshape([batch, seq_len, codebooks, HEARTMULA_HIDDEN_SIZE])
    }

    fn embed_offset_token(&self, device: &B::Device, offset_token: i64) -> Tensor<B, 3> {
        debug_assert!((offset_token as usize) < self.vocab_size * HEARTMULA_AUDIO_CODEBOOKS);
        let ids = Tensor::<B, 1, Int>::from_data([offset_token], device);
        self.table
            .as_ref()
            .expect("audio embeddings must be loaded before use")
            .clone()
            .select(0, ids)
            .reshape([1, 1, HEARTMULA_HIDDEN_SIZE])
    }
}

impl<B: Backend> Module<B> for SplitAudioEmbeddings<B> {
    type Record = ConstantRecord;

    fn visit<V: ModuleVisitor<B>>(&self, _visitor: &mut V) {}

    fn map<M: ModuleMapper<B>>(self, _mapper: &mut M) -> Self {
        self
    }

    fn load_record(self, _record: Self::Record) -> Self {
        self
    }

    fn into_record(self) -> Self::Record {
        ConstantRecord::new()
    }

    fn to_device(self, device: &B::Device) -> Self {
        Self {
            table: self.table.map(|tensor| tensor.to_device(device)),
            vocab_size: self.vocab_size,
        }
    }

    fn fork(self, device: &B::Device) -> Self {
        Self {
            table: self.table.map(|tensor| tensor.fork(device)),
            vocab_size: self.vocab_size,
        }
    }

    fn collect_devices(&self, mut devices: Devices<B>) -> Devices<B> {
        if let Some(tensor) = &self.table {
            let device = tensor.device();
            if !devices.contains(&device) {
                devices.push(device);
            }
        }
        devices
    }
}

impl<B: Backend> ModuleDisplayDefault for SplitAudioEmbeddings<B> {
    fn content(&self, content: Content) -> Option<Content> {
        content
            .add("table_loaded", &self.table.is_some())
            .add("vocab_size", &self.vocab_size)
            .optional()
    }
}

impl<B: Backend> ModuleDisplay for SplitAudioEmbeddings<B> {}

impl<B: AutodiffBackend> AutodiffModule<B> for SplitAudioEmbeddings<B> {
    type InnerModule = SplitAudioEmbeddings<B::InnerBackend>;

    fn valid(&self) -> Self::InnerModule {
        SplitAudioEmbeddings {
            table: self.table.as_ref().map(|tensor| tensor.valid()),
            vocab_size: self.vocab_size,
        }
    }

    fn from_inner(module: Self::InnerModule) -> Self {
        SplitAudioEmbeddings {
            table: module
                .table
                .map(|tensor| Tensor::<B, 2>::from_data(tensor.to_data(), &tensor.device())),
            vocab_size: module.vocab_size,
        }
    }
}

impl<B: Backend> HeartmulaTransformer<B> {
    fn new(device: &B::Device, layer_count: usize, num_heads: usize, num_kv_heads: usize) -> Self {
        let layers = (0..layer_count)
            .map(|_| HeartmulaTransformerLayer::new(device, num_heads, num_kv_heads))
            .collect();
        Self {
            layers,
            norm: HeartmulaRmsNorm::new(device, HEARTMULA_HIDDEN_SIZE, HEARTMULA_NORM_EPSILON),
        }
    }

    #[allow(dead_code)]
    fn forward(
        &self,
        mut hidden: Tensor<B, 3>,
        positions: Tensor<B, 2, Int>,
    ) -> Result<Tensor<B, 3>> {
        for layer in &self.layers {
            hidden = layer.forward(hidden, positions.clone())?;
        }
        Ok(self.norm.forward(hidden))
    }

    fn new_cache(&self) -> HeartmulaTransformerCache<B> {
        HeartmulaTransformerCache {
            layers: (0..self.layers.len())
                .map(|_| HeartmulaAttentionCache {
                    key: None,
                    value: None,
                })
                .collect(),
        }
    }

    fn forward_incremental(
        &self,
        mut hidden: Tensor<B, 3>,
        position: Tensor<B, 2, Int>,
        cache: &mut HeartmulaTransformerCache<B>,
    ) -> Result<Tensor<B, 2>> {
        for (layer, layer_cache) in self.layers.iter().zip(cache.layers.iter_mut()) {
            hidden = layer.forward_incremental(hidden, position.clone(), layer_cache)?;
        }
        Ok(take_last_token(self.norm.forward(hidden)))
    }

    fn forward_prefill(
        &self,
        mut hidden: Tensor<B, 3>,
        positions: Tensor<B, 2, Int>,
        cache: &mut HeartmulaTransformerCache<B>,
    ) -> Result<Tensor<B, 2>> {
        for (layer, layer_cache) in self.layers.iter().zip(cache.layers.iter_mut()) {
            hidden = layer.forward_prefill(hidden, positions.clone(), layer_cache)?;
        }
        Ok(take_last_token(self.norm.forward(hidden)))
    }
}

impl<B: Backend> HeartmulaTransformerLayer<B> {
    fn new(device: &B::Device, num_heads: usize, num_kv_heads: usize) -> Self {
        Self {
            attn: HeartmulaAttention::new(device, num_heads, num_kv_heads),
            mlp: HeartmulaMlp::new(device),
            sa_norm: HeartmulaRmsNorm::new(device, HEARTMULA_HIDDEN_SIZE, HEARTMULA_NORM_EPSILON),
            mlp_norm: HeartmulaRmsNorm::new(device, HEARTMULA_HIDDEN_SIZE, HEARTMULA_NORM_EPSILON),
        }
    }

    #[allow(dead_code)]
    fn forward(&self, hidden: Tensor<B, 3>, positions: Tensor<B, 2, Int>) -> Result<Tensor<B, 3>> {
        let attn_hidden = self
            .attn
            .forward(self.sa_norm.forward(hidden.clone()), positions)?;
        let hidden = hidden + attn_hidden;
        let mlp_hidden = self.mlp.forward(self.mlp_norm.forward(hidden.clone()));
        Ok(hidden + mlp_hidden)
    }

    fn forward_incremental(
        &self,
        hidden: Tensor<B, 3>,
        position: Tensor<B, 2, Int>,
        cache: &mut HeartmulaAttentionCache<B>,
    ) -> Result<Tensor<B, 3>> {
        let attn_hidden =
            self.attn
                .forward_incremental(self.sa_norm.forward(hidden.clone()), position, cache)?;
        let hidden = hidden + attn_hidden;
        let mlp_hidden = self.mlp.forward(self.mlp_norm.forward(hidden.clone()));
        Ok(hidden + mlp_hidden)
    }

    fn forward_prefill(
        &self,
        hidden: Tensor<B, 3>,
        positions: Tensor<B, 2, Int>,
        cache: &mut HeartmulaAttentionCache<B>,
    ) -> Result<Tensor<B, 3>> {
        let attn_hidden =
            self.attn
                .forward_prefill(self.sa_norm.forward(hidden.clone()), positions, cache)?;
        let hidden = hidden + attn_hidden;
        let mlp_hidden = self.mlp.forward(self.mlp_norm.forward(hidden.clone()));
        Ok(hidden + mlp_hidden)
    }
}

impl<B: Backend> HeartmulaAttention<B> {
    fn new(device: &B::Device, num_heads: usize, num_kv_heads: usize) -> Self {
        let head_dim = HEARTMULA_HIDDEN_SIZE / num_heads;
        Self {
            q_proj: linear_no_bias(device, HEARTMULA_HIDDEN_SIZE, num_heads * head_dim),
            k_proj: linear_no_bias(device, HEARTMULA_HIDDEN_SIZE, num_kv_heads * head_dim),
            v_proj: linear_no_bias(device, HEARTMULA_HIDDEN_SIZE, num_kv_heads * head_dim),
            output_proj: linear_no_bias(device, HEARTMULA_HIDDEN_SIZE, HEARTMULA_HIDDEN_SIZE),
            meta: Ignored(AttentionMeta {
                num_heads,
                num_kv_heads,
                head_dim,
            }),
        }
    }

    #[allow(dead_code)]
    fn forward(&self, hidden: Tensor<B, 3>, positions: Tensor<B, 2, Int>) -> Result<Tensor<B, 3>> {
        let [batch, seq_len, _] = hidden.dims();
        let q = self.q_proj.forward(hidden.clone()).reshape([
            batch,
            seq_len,
            self.meta.0.num_heads,
            self.meta.0.head_dim,
        ]);
        let k = self.k_proj.forward(hidden.clone()).reshape([
            batch,
            seq_len,
            self.meta.0.num_kv_heads,
            self.meta.0.head_dim,
        ]);
        let v = self.v_proj.forward(hidden).reshape([
            batch,
            seq_len,
            self.meta.0.num_kv_heads,
            self.meta.0.head_dim,
        ]);

        let q = apply_scaled_rope(q, &positions);
        let k = apply_scaled_rope(k, &positions);
        let (k, v) = if self.meta.0.num_heads != self.meta.0.num_kv_heads {
            let repeats = self.meta.0.num_heads / self.meta.0.num_kv_heads;
            (repeat_kv_heads(k, repeats), repeat_kv_heads(v, repeats))
        } else {
            (k, v)
        };

        let q = q.swap_dims(1, 2);
        let k = k.swap_dims(1, 2);
        let v = v.swap_dims(1, 2);
        let scores = q
            .matmul(k.clone().swap_dims(2, 3))
            .mul_scalar(1.0 / (self.meta.0.head_dim as f32).sqrt());
        let mask = causal_mask::<B>(seq_len, &scores.device());
        let weights = softmax(scores.mask_fill(mask, -1.0e9), 3);
        let attended =
            weights
                .matmul(v)
                .swap_dims(1, 2)
                .reshape([batch, seq_len, HEARTMULA_HIDDEN_SIZE]);
        Ok(self.output_proj.forward(attended))
    }

    fn forward_incremental(
        &self,
        hidden: Tensor<B, 3>,
        position: Tensor<B, 2, Int>,
        cache: &mut HeartmulaAttentionCache<B>,
    ) -> Result<Tensor<B, 3>> {
        let [batch, seq_len, _] = hidden.dims();
        let q = self.q_proj.forward(hidden.clone()).reshape([
            batch,
            seq_len,
            self.meta.0.num_heads,
            self.meta.0.head_dim,
        ]);
        let k = self.k_proj.forward(hidden.clone()).reshape([
            batch,
            seq_len,
            self.meta.0.num_kv_heads,
            self.meta.0.head_dim,
        ]);
        let v = self.v_proj.forward(hidden).reshape([
            batch,
            seq_len,
            self.meta.0.num_kv_heads,
            self.meta.0.head_dim,
        ]);

        let q = apply_scaled_rope(q, &position).swap_dims(1, 2);
        let k = apply_scaled_rope(k, &position).swap_dims(1, 2);
        let v = v.swap_dims(1, 2);

        let full_k = if let Some(previous) = &cache.key {
            Tensor::cat(vec![previous.clone(), k], 2)
        } else {
            k
        };
        let full_v = if let Some(previous) = &cache.value {
            Tensor::cat(vec![previous.clone(), v], 2)
        } else {
            v
        };
        cache.key = Some(full_k.clone());
        cache.value = Some(full_v.clone());

        let (full_k_for_attn, full_v_for_attn) =
            if self.meta.0.num_heads != self.meta.0.num_kv_heads {
                let repeats = self.meta.0.num_heads / self.meta.0.num_kv_heads;
                (
                    repeat_cached_kv_heads(full_k, repeats),
                    repeat_cached_kv_heads(full_v, repeats),
                )
            } else {
                (full_k, full_v)
            };

        let weights = softmax(
            q.matmul(full_k_for_attn.swap_dims(2, 3))
                .mul_scalar(1.0 / (self.meta.0.head_dim as f32).sqrt()),
            3,
        );
        let attended = weights.matmul(full_v_for_attn).swap_dims(1, 2).reshape([
            batch,
            seq_len,
            HEARTMULA_HIDDEN_SIZE,
        ]);
        Ok(self.output_proj.forward(attended))
    }

    fn forward_prefill(
        &self,
        hidden: Tensor<B, 3>,
        positions: Tensor<B, 2, Int>,
        cache: &mut HeartmulaAttentionCache<B>,
    ) -> Result<Tensor<B, 3>> {
        let [batch, seq_len, _] = hidden.dims();
        let q = self.q_proj.forward(hidden.clone()).reshape([
            batch,
            seq_len,
            self.meta.0.num_heads,
            self.meta.0.head_dim,
        ]);
        let k = self.k_proj.forward(hidden.clone()).reshape([
            batch,
            seq_len,
            self.meta.0.num_kv_heads,
            self.meta.0.head_dim,
        ]);
        let v = self.v_proj.forward(hidden).reshape([
            batch,
            seq_len,
            self.meta.0.num_kv_heads,
            self.meta.0.head_dim,
        ]);

        let q = apply_scaled_rope(q, &positions).swap_dims(1, 2);
        let k = apply_scaled_rope(k, &positions).swap_dims(1, 2);
        let v = v.swap_dims(1, 2);
        cache.key = Some(k.clone());
        cache.value = Some(v.clone());

        let (k_for_attn, v_for_attn) = if self.meta.0.num_heads != self.meta.0.num_kv_heads {
            let repeats = self.meta.0.num_heads / self.meta.0.num_kv_heads;
            (
                repeat_cached_kv_heads(k, repeats),
                repeat_cached_kv_heads(v, repeats),
            )
        } else {
            (k, v)
        };

        let scores = q
            .matmul(k_for_attn.clone().swap_dims(2, 3))
            .mul_scalar(1.0 / (self.meta.0.head_dim as f32).sqrt());
        let mask = causal_mask::<B>(seq_len, &scores.device());
        let weights = softmax(scores.mask_fill(mask, -1.0e9), 3);
        let attended = weights.matmul(v_for_attn).swap_dims(1, 2).reshape([
            batch,
            seq_len,
            HEARTMULA_HIDDEN_SIZE,
        ]);
        Ok(self.output_proj.forward(attended))
    }
}

impl<B: Backend> HeartmulaMlp<B> {
    fn new(device: &B::Device) -> Self {
        Self {
            w1: linear_no_bias(device, HEARTMULA_HIDDEN_SIZE, HEARTMULA_MLP_DIM),
            w2: linear_no_bias(device, HEARTMULA_MLP_DIM, HEARTMULA_HIDDEN_SIZE),
            w3: linear_no_bias(device, HEARTMULA_HIDDEN_SIZE, HEARTMULA_MLP_DIM),
        }
    }

    fn forward(&self, hidden: Tensor<B, 3>) -> Tensor<B, 3> {
        let gate = silu(self.w1.forward(hidden.clone()));
        let up = self.w3.forward(hidden);
        self.w2.forward(gate * up)
    }
}

impl<B: Backend> HeartmulaRmsNorm<B> {
    fn new(device: &B::Device, hidden_size: usize, epsilon: f64) -> Self {
        Self {
            scale: Param::from_tensor(Tensor::<B, 1>::ones([hidden_size], device)),
            epsilon,
        }
    }

    fn forward<const D: usize>(&self, hidden: Tensor<B, D>) -> Tensor<B, D> {
        let dtype = hidden.dtype();
        let rms = (hidden.clone().cast(DType::F32).square().mean_dim(D - 1) + self.epsilon).sqrt();
        (hidden / rms.cast(dtype)) * self.scale.val().unsqueeze()
    }
}

pub fn tokenize_text(tokenizer_json: &Path, text: &str) -> Result<Vec<i64>> {
    let tokenizer = Tokenizer::from_file(tokenizer_json).map_err(|e| {
        anyhow!(
            "failed to load tokenizer from {}: {e}",
            tokenizer_json.display()
        )
    })?;
    let encoding = tokenizer.encode(text, true).map_err(|e| {
        anyhow!(
            "failed to encode text with {}: {e}",
            tokenizer_json.display()
        )
    })?;
    Ok(encoding.get_ids().iter().map(|&id| i64::from(id)).collect())
}

pub fn default_tags() -> &'static str {
    "<tag></tag>"
}

pub fn normalize_tags(tags: &str) -> String {
    let mut normalized = tags.trim().to_lowercase();
    // Remove all spaces after commas (handles multiple spaces)
    while normalized.contains(", ") {
        normalized = normalized.replace(", ", ",");
    }
    if !normalized.starts_with("<tag>") {
        normalized = format!("<tag>{normalized}");
    }
    if !normalized.ends_with("</tag>") {
        normalized.push_str("</tag>");
    }
    normalized
}

pub fn write_frames_json(path: &Path, lyrics: &str, tags: &str, frames: &[Vec<i64>]) -> Result<()> {
    let payload = HeartmulaJsonOutput {
        model: "heartmula".to_string(),
        runtime: "burn-token-generator".to_string(),
        tags: tags.to_owned(),
        lyrics: lyrics.to_owned(),
        frames: frames.to_vec(),
        frame_count: frames.len(),
        sample_rate_hz: 48_000,
    };
    std::fs::write(path, serde_json::to_vec_pretty(&payload)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Decode frames to WAV using Rust HeartCodec implementation
#[allow(clippy::too_many_arguments)]
pub fn decode_frames_to_wav<B: burn::prelude::Backend>(
    model_dir: &Path,
    _backend_arg: &str,
    float_size_arg: &str,
    frames_json: &Path,
    output_wav: &Path,
    duration_seconds: f32,
    device: &B::Device,
    ode_steps: usize,
    decoder_seed: u64,
) -> Result<()> {
    if let Some(stage) =
        env::var_os(HEARTCODEC_STAGE_ENV).and_then(|value| value.into_string().ok())
    {
        return match stage.as_str() {
            HEARTCODEC_STAGE_FLOW => decode_frames_to_plan_rust::<B>(
                model_dir,
                frames_json,
                device,
                ode_steps,
                &prepare_shared_decoder_initial_latent(frames_json, decoder_seed, output_wav)?,
            ),
            HEARTCODEC_STAGE_SCALAR => decode_plan_to_wav_rust::<B>(model_dir, output_wav, device),
            other => Err(anyhow!("unsupported HeartCodec stage '{other}'")),
        };
    }

    let _ = float_size_arg;
    let _ = duration_seconds;
    eprintln!("decode_frames_to_wav: using Rust HeartCodec decoder");
    decode_frames_to_wav_rust::<B>(
        model_dir,
        frames_json,
        output_wav,
        duration_seconds,
        decoder_seed,
        device,
        ode_steps,
    )
}

fn resolve_heartcodec_burnpack_path(model_dir: &Path) -> PathBuf {
    model_dir.join("heartcodec.bpk")
}

fn decode_frames_to_wav_rust<B: burn::prelude::Backend>(
    model_dir: &Path,
    frames_json: &Path,
    output_wav: &Path,
    duration_seconds: f32,
    decoder_seed: u64,
    device: &B::Device,
    ode_steps: usize,
) -> Result<()> {
    let frames_text = std::fs::read_to_string(frames_json)
        .with_context(|| format!("failed to read {}", frames_json.display()))?;
    let _payload: HeartmulaJsonOutput = serde_json::from_str(&frames_text)
        .with_context(|| format!("failed to parse {}", frames_json.display()))?;
    eprintln!("decode_frames_to_wav_rust: seeding backend RNG with 0");
    B::seed(device, 0);
    eprintln!(
        "decode_frames_to_wav_rust: model_dir={}",
        model_dir.display()
    );
    eprintln!("decode_frames_to_wav_rust: staging flow-matching decode plan");
    let initial_latent_json =
        prepare_shared_decoder_initial_latent(frames_json, decoder_seed, output_wav)?;
    let stage_plan_json = output_wav.with_extension("heartcodec-stage-plan.bin");

    unsafe {
        std::env::set_var(HEARTCODEC_STAGE_PLAN_JSON_ENV, &stage_plan_json);
    }

    let plan_result = decode_frames_to_plan_rust::<B>(
        model_dir,
        frames_json,
        device,
        ode_steps,
        &initial_latent_json,
    );
    sync_and_cleanup_backend::<B>(device)?;
    plan_result?;

    eprintln!("decode_frames_to_wav_rust: flow-matching plan complete; starting scalar decode");
    let decode_result = decode_plan_to_wav_rust::<B>(model_dir, output_wav, device);
    sync_and_cleanup_backend::<B>(device)?;

    let _ = std::fs::remove_file(&stage_plan_json);
    let _ = std::fs::remove_file(&initial_latent_json);

    decode_result?;
    eprintln!("decode_frames_to_wav_rust: staged decode complete");
    let _ = duration_seconds;
    Ok(())
}

pub fn decode_frames_to_plan_rust<B: burn::prelude::Backend>(
    model_dir: &Path,
    frames_json: &Path,
    device: &B::Device,
    ode_steps: usize,
    initial_latent_json: &Path,
) -> Result<()> {
    let frames_text = std::fs::read_to_string(frames_json)
        .with_context(|| format!("failed to read {}", frames_json.display()))?;
    let payload: HeartmulaJsonOutput = serde_json::from_str(&frames_text)
        .with_context(|| format!("failed to parse {}", frames_json.display()))?;
    let frames = payload.frames;
    eprintln!("decode_frames_to_wav_rust: seeding backend RNG with 0");
    B::seed(device, 0);
    let codes = frames_to_tensor::<B>(&frames, device);
    let codec_path = resolve_heartcodec_burnpack_path(model_dir);
    eprintln!(
        "decode_frames_to_wav_rust: model_dir={}",
        model_dir.display()
    );
    eprintln!(
        "decode_frames_to_wav_rust: codec_path={}",
        codec_path.display()
    );
    let flow_matching =
        crate::heartcodec::FlowMatching::<B>::load_from_burnpack(&codec_path, device)?;
    let initial_latent = load_initial_latent_tensor::<B>(initial_latent_json, device)?;
    let plan = crate::heartcodec::HeartCodecModel::<B>::build_scalar_decode_plan_impl(
        &flow_matching,
        1.25,
        ode_steps,
        codes,
        initial_latent,
    );
    let stage_plan_json = current_codec_stage_plan_json()?;
    save_codec_stage_plan(&stage_plan_json, plan)?;
    Ok(())
}

pub fn decode_plan_to_wav_rust<B: burn::prelude::Backend>(
    model_dir: &Path,
    output_wav: &Path,
    device: &B::Device,
) -> Result<()> {
    let stage_plan_json = current_codec_stage_plan_json()?;
    let plan = load_codec_stage_plan::<B>(&stage_plan_json, device)?;
    let codec_path = resolve_heartcodec_burnpack_path(model_dir);
    let scalar_model = crate::heartcodec::ScalarModel::<B>::from_burnpack(&codec_path, device)?;
    let wav = crate::heartcodec::HeartCodecModel::<B>::decode_scalar_plan_impl(&scalar_model, plan);
    write_decoder_wav(output_wav, wav, 0.0)
}

fn write_decoder_wav<B: burn::prelude::Backend>(
    output_wav: &Path,
    wav: Tensor<B, 3>,
    _duration_seconds: f32,
) -> Result<()> {
    let dims = wav.dims();
    let samples: Vec<f32> = wav.cast(DType::F32).to_data().to_vec::<f32>()?;
    eprintln!(
        "decode_frames_to_wav_rust: wav dims={:?} samples_len={}",
        dims,
        samples.len()
    );
    match dims.as_slice() {
        [channels, 1, frames] if *channels > 1 => {
            eprintln!(
                "decode_frames_to_wav_rust: writing interleaved stereo/multichannel wav channels={} frames={}",
                channels, frames
            );
            crate::heartcodec::write_wav_from_f32_interleaved(
                &samples, *channels, *frames, 48_000, output_wav,
            )
        }
        [1, channels, frames] if *channels > 1 => {
            eprintln!(
                "decode_frames_to_wav_rust: writing interleaved wav from [1, channels, frames] channels={} frames={}",
                channels, frames
            );
            crate::heartcodec::write_wav_from_f32_interleaved(
                &samples, *channels, *frames, 48_000, output_wav,
            )
        }
        [1, 1, frames] => {
            eprintln!(
                "decode_frames_to_wav_rust: writing mono wav frames={}",
                frames
            );
            crate::heartcodec::write_wav_from_f32(&samples[..*frames], 48_000, output_wav)
        }
        _ => {
            eprintln!("decode_frames_to_wav_rust: writing fallback mono wav");
            crate::heartcodec::write_wav_from_f32(&samples, 48_000, output_wav)
        }
    }
}

fn current_codec_stage_plan_json() -> Result<PathBuf> {
    env::var_os(HEARTCODEC_STAGE_PLAN_JSON_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| {
            anyhow!("missing {HEARTCODEC_STAGE_PLAN_JSON_ENV} for staged HeartCodec decode")
        })
}

fn save_codec_stage_plan<B: burn::prelude::Backend>(
    path: &Path,
    plan: crate::heartcodec::ScalarDecodePlan<B>,
) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(HEARTCODEC_STAGE_PLAN_MAGIC)
        .with_context(|| format!("failed to write {}", path.display()))?;
    write_u64(&mut writer, plan.target_len)?;
    write_u64(&mut writer, plan.audio_target_len)?;
    write_u64(&mut writer, plan.windows.len())?;
    for window in plan.windows {
        let dims = window.dims();
        let data = window.cast(DType::F32).to_data().to_vec::<f32>()?;
        write_dims(&mut writer, dims)?;
        write_f32_slice(&mut writer, &data)?;
    }
    writer
        .flush()
        .with_context(|| format!("failed to flush {}", path.display()))
}

fn load_codec_stage_plan<B: burn::prelude::Backend>(
    path: &Path,
    device: &B::Device,
) -> Result<crate::heartcodec::ScalarDecodePlan<B>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut magic = [0_u8; HEARTCODEC_STAGE_PLAN_MAGIC.len()];
    reader
        .read_exact(&mut magic)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if &magic != HEARTCODEC_STAGE_PLAN_MAGIC {
        anyhow::bail!("invalid HeartCodec stage plan format in {}", path.display());
    }
    let target_len = read_u64(&mut reader)? as usize;
    let audio_target_len = read_u64(&mut reader)? as usize;
    let window_count = read_u64(&mut reader)? as usize;
    let mut windows = Vec::with_capacity(window_count);
    for _ in 0..window_count {
        let dims = read_dims(&mut reader)?;
        let data = read_f32_vec(&mut reader)?;
        windows.push(Tensor::<B, 3>::from_data(
            TensorData::new(data, dims),
            device,
        ));
    }
    Ok(crate::heartcodec::ScalarDecodePlan {
        target_len,
        audio_target_len,
        windows,
    })
}

fn write_u64(writer: &mut dyn Write, value: usize) -> Result<()> {
    writer.write_all(&(value as u64).to_le_bytes())?;
    Ok(())
}

fn read_u64(reader: &mut dyn Read) -> Result<u64> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn write_dims(writer: &mut dyn Write, dims: [usize; 3]) -> Result<()> {
    for value in dims {
        write_u64(writer, value)?;
    }
    Ok(())
}

fn read_dims(reader: &mut dyn Read) -> Result<[usize; 3]> {
    Ok([
        read_u64(reader)? as usize,
        read_u64(reader)? as usize,
        read_u64(reader)? as usize,
    ])
}

fn write_f32_slice(writer: &mut dyn Write, values: &[f32]) -> Result<()> {
    write_u64(writer, values.len())?;
    let mut bytes = vec![0_u8; std::mem::size_of_val(values)];
    bytes
        .par_chunks_mut(std::mem::size_of::<f32>())
        .zip(values.par_iter())
        .for_each(|(chunk, value)| chunk.copy_from_slice(&value.to_le_bytes()));
    writer.write_all(&bytes)?;
    Ok(())
}

fn read_f32_vec(reader: &mut dyn Read) -> Result<Vec<f32>> {
    let len = read_u64(reader)? as usize;
    let mut bytes = vec![0_u8; len * std::mem::size_of::<f32>()];
    reader.read_exact(&mut bytes)?;
    let mut values = vec![0.0_f32; len];
    values
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, value)| {
            let offset = index * std::mem::size_of::<f32>();
            *value = f32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
        });
    Ok(values)
}

fn prepare_shared_decoder_initial_latent(
    _frames_json: &Path,
    decoder_seed: u64,
    output_wav: &Path,
) -> Result<PathBuf> {
    let latent_length = (HEARTCODEC_SEGMENT_DURATION_SECONDS * 25.0) as usize;
    let dims = [1, latent_length, 256];
    let data = generate_decoder_latent_data(decoder_seed, dims[0] * dims[1] * dims[2]);
    let latent = LatentTensorFile { dims, data };

    let stem = output_wav
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("decoder");
    let path = std::env::temp_dir().join(format!(
        "maolan-{stem}-decoder-seed-{decoder_seed}-latent-{latent_length}.json"
    ));
    fs::write(&path, serde_json::to_vec(&latent)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn load_initial_latent_tensor<B: burn::prelude::Backend>(
    path: &Path,
    device: &B::Device,
) -> Result<Tensor<B, 3>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let payload: LatentTensorFile = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Tensor::<B, 3>::from_data(
        TensorData::new(payload.data, payload.dims),
        device,
    ))
}

fn generate_decoder_latent_data(seed: u64, len: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(len);
    let mut state = seed;
    while out.len() < len {
        let u1 = uniform01_open(&mut state);
        let u2 = uniform01_open(&mut state);
        let radius = (-2.0_f64 * u1.ln()).sqrt();
        let theta = 2.0_f64 * std::f64::consts::PI * u2;
        out.push((radius * theta.cos()) as f32);
        if out.len() < len {
            out.push((radius * theta.sin()) as f32);
        }
    }
    out
}

fn uniform01_open(state: &mut u64) -> f64 {
    let value = splitmix64_next(state);
    let mantissa = (value >> 11) as f64;
    ((mantissa + 0.5) / ((1_u64 << 53) as f64)).clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON)
}

fn splitmix64_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn build_prompt_history(
    text_bos_id: i64,
    text_eos_id: i64,
    lyrics_ids: &[i64],
    tags_ids: &[i64],
) -> Vec<[i64; HEARTMULA_PARALLEL_TOKENS]> {
    let full_tags = normalize_text_ids(text_bos_id, text_eos_id, tags_ids);
    let full_lyrics = normalize_text_ids(text_bos_id, text_eos_id, lyrics_ids);

    let mut history = Vec::with_capacity(full_tags.len() + 1 + full_lyrics.len());
    for token in full_tags {
        let mut row = [0_i64; HEARTMULA_PARALLEL_TOKENS];
        row[HEARTMULA_AUDIO_CODEBOOKS] = token;
        history.push(row);
    }
    history.push([0_i64; HEARTMULA_PARALLEL_TOKENS]);
    for token in full_lyrics {
        let mut row = [0_i64; HEARTMULA_PARALLEL_TOKENS];
        row[HEARTMULA_AUDIO_CODEBOOKS] = token;
        history.push(row);
    }
    history
}

fn normalize_text_ids(text_bos_id: i64, text_eos_id: i64, ids: &[i64]) -> Vec<i64> {
    let mut normalized = ids.to_vec();
    if normalized.first().copied() != Some(text_bos_id) {
        normalized.insert(0, text_bos_id);
    }
    if normalized.last().copied() != Some(text_eos_id) {
        normalized.push(text_eos_id);
    }
    normalized
}

fn build_audio_history_row(frame: &[i64], empty_id: i64) -> [i64; HEARTMULA_PARALLEL_TOKENS] {
    let mut row = [empty_id; HEARTMULA_PARALLEL_TOKENS];
    for (index, token) in frame
        .iter()
        .copied()
        .enumerate()
        .take(HEARTMULA_AUDIO_CODEBOOKS)
    {
        row[index] = token;
    }
    row[HEARTMULA_AUDIO_CODEBOOKS] = empty_id;
    row
}

fn splice_sequence_token<B: Backend>(
    hidden: Tensor<B, 3>,
    replacement: Tensor<B, 3>,
    index: usize,
) -> Tensor<B, 3> {
    let [batch, seq_len, dim] = hidden.dims();
    debug_assert_eq!(replacement.dims(), [batch, 1, dim]);
    let mut parts = Vec::new();
    if index > 0 {
        parts.push(hidden.clone().slice([0..batch, 0..index, 0..dim]));
    }
    parts.push(replacement);
    if index + 1 < seq_len {
        parts.push(hidden.slice([0..batch, index + 1..seq_len, 0..dim]));
    }
    Tensor::cat(parts, 1)
}

fn history_tokens_tensor<B: Backend>(
    history: &[[i64; HEARTMULA_PARALLEL_TOKENS]],
    device: &B::Device,
) -> Tensor<B, 3, Int> {
    let flattened = history
        .iter()
        .flat_map(|row| row.iter().copied())
        .collect::<Vec<_>>();
    Tensor::<B, 3, Int>::from_data(
        TensorData::new(flattened, [1, history.len(), HEARTMULA_PARALLEL_TOKENS]),
        device,
    )
}

fn history_mask_tensor<B: Backend>(
    history: &[[i64; HEARTMULA_PARALLEL_TOKENS]],
    device: &B::Device,
) -> Tensor<B, 3, Bool> {
    let flattened = history
        .iter()
        .flat_map(|row| {
            let has_audio_tokens = row[..HEARTMULA_AUDIO_CODEBOOKS]
                .iter()
                .any(|token| *token != 0);
            row.iter().enumerate().map(move |(index, token)| {
                if index < HEARTMULA_AUDIO_CODEBOOKS {
                    *token != 0
                } else if index == HEARTMULA_AUDIO_CODEBOOKS {
                    !has_audio_tokens
                } else {
                    false
                }
            })
        })
        .collect::<Vec<_>>();
    Tensor::<B, 3, Bool>::from_data(
        TensorData::new(flattened, [1, history.len(), HEARTMULA_PARALLEL_TOKENS]),
        device,
    )
}

#[allow(dead_code)]
fn positions_tensor<B: Backend>(seq_len: usize, device: &B::Device) -> Tensor<B, 2, Int> {
    Tensor::<B, 1, Int>::arange(0..seq_len as i64, device).reshape([1, seq_len])
}

fn single_position_tensor<B: Backend>(position: i64, device: &B::Device) -> Tensor<B, 2, Int> {
    Tensor::<B, 2, Int>::from_data([[position]], device)
}

fn position_tensor<B: Backend>(positions: Vec<i64>, device: &B::Device) -> Tensor<B, 2, Int> {
    let len = positions.len();
    Tensor::<B, 1, Int>::from_data(TensorData::new(positions, [len]), device).reshape([1, len])
}

fn repeat_kv_heads<B: Backend>(tensor: Tensor<B, 4>, repeats: usize) -> Tensor<B, 4> {
    let [batch, seq_len, heads, head_dim] = tensor.dims();
    tensor
        .unsqueeze_dim::<5>(3)
        .repeat_dim(3, repeats)
        .reshape([batch, seq_len, heads * repeats, head_dim])
}

fn repeat_cached_kv_heads<B: Backend>(tensor: Tensor<B, 4>, repeats: usize) -> Tensor<B, 4> {
    let [batch, heads, seq_len, head_dim] = tensor.dims();
    tensor
        .unsqueeze_dim::<5>(2)
        .repeat_dim(2, repeats)
        .reshape([batch, heads * repeats, seq_len, head_dim])
}

fn apply_scaled_rope<B: Backend>(
    tensor: Tensor<B, 4>,
    positions: &Tensor<B, 2, Int>,
) -> Tensor<B, 4> {
    let [batch, seq_len, num_heads, head_dim] = tensor.dims();
    let pos = positions
        .clone()
        .to_data()
        .to_vec::<i64>()
        .expect("positions should be materializable");
    let cache = scaled_rope_cache::<B>(&tensor.device(), &pos, head_dim)
        .reshape([1, seq_len, 1, head_dim / 2, 2])
        .repeat_dim(0, batch);
    let reshaped = tensor.reshape([batch, seq_len, num_heads, head_dim / 2, 2]);
    Tensor::cat(
        vec![
            (reshaped
                .clone()
                .slice([0..batch, 0..seq_len, 0..num_heads, 0..head_dim / 2, 0..1])
                * cache
                    .clone()
                    .slice([0..batch, 0..seq_len, 0..1, 0..head_dim / 2, 0..1]))
                - (reshaped.clone().slice([
                    0..batch,
                    0..seq_len,
                    0..num_heads,
                    0..head_dim / 2,
                    1..2,
                ]) * cache
                    .clone()
                    .slice([0..batch, 0..seq_len, 0..1, 0..head_dim / 2, 1..2])),
            (reshaped
                .clone()
                .slice([0..batch, 0..seq_len, 0..num_heads, 0..head_dim / 2, 1..2])
                * cache
                    .clone()
                    .slice([0..batch, 0..seq_len, 0..1, 0..head_dim / 2, 0..1]))
                + (reshaped.slice([0..batch, 0..seq_len, 0..num_heads, 0..head_dim / 2, 0..1])
                    * cache.slice([0..batch, 0..seq_len, 0..1, 0..head_dim / 2, 1..2])),
        ],
        4,
    )
    .reshape([batch, seq_len, num_heads, head_dim])
}

fn scaled_rope_cache<B: Backend>(
    device: &B::Device,
    positions: &[i64],
    head_dim: usize,
) -> Tensor<B, 3> {
    let theta = scaled_theta(head_dim);
    let mut values = Vec::with_capacity(positions.len() * (head_dim / 2) * 2);
    for &pos in positions {
        for &freq in &theta {
            let angle = pos as f32 * freq;
            values.push(angle.cos());
            values.push(angle.sin());
        }
    }
    Tensor::<B, 3>::from_data(
        TensorData::new(values, [positions.len(), head_dim / 2, 2]),
        device,
    )
}

fn scaled_theta(head_dim: usize) -> Vec<f32> {
    (0..head_dim)
        .step_by(2)
        .map(|index| {
            let exponent = index as f32 / head_dim as f32;
            let freq = HEARTMULA_ROPE_BASE.powf(-exponent);
            let wavelength = 2.0 * std::f32::consts::PI / freq;
            let low_freq_wavelen = HEARTMULA_OLD_CONTEXT_LEN / HEARTMULA_LOW_FREQ_FACTOR;
            let high_freq_wavelen = HEARTMULA_OLD_CONTEXT_LEN / HEARTMULA_HIGH_FREQ_FACTOR;
            if wavelength < high_freq_wavelen {
                freq
            } else if wavelength > low_freq_wavelen {
                freq / HEARTMULA_ROPE_SCALE_FACTOR
            } else {
                let smooth = (HEARTMULA_OLD_CONTEXT_LEN / wavelength - HEARTMULA_LOW_FREQ_FACTOR)
                    / (HEARTMULA_HIGH_FREQ_FACTOR - HEARTMULA_LOW_FREQ_FACTOR);
                (1.0 - smooth) * freq / HEARTMULA_ROPE_SCALE_FACTOR + smooth * freq
            }
        })
        .collect()
}

#[allow(dead_code)]
fn causal_mask<B: Backend>(seq_len: usize, device: &B::Device) -> Tensor<B, 4, Bool> {
    let mut mask = Vec::with_capacity(seq_len * seq_len);
    for row in 0..seq_len {
        for col in 0..seq_len {
            mask.push(col > row);
        }
    }
    Tensor::<B, 4, Bool>::from_data(TensorData::new(mask, [1, 1, seq_len, seq_len]), device)
}

fn take_last_token<B: Backend>(hidden: Tensor<B, 3>) -> Tensor<B, 2> {
    let [batch, seq_len, hidden_size] = hidden.dims();
    hidden
        .slice([0..batch, seq_len - 1..seq_len, 0..hidden_size])
        .reshape([batch, hidden_size])
}

fn tensor_to_f32_vec<B: Backend, const D: usize>(tensor: Tensor<B, D>) -> Result<Vec<f32>> {
    tensor
        .cast(DType::F32)
        .to_data()
        .to_vec::<f32>()
        .map_err(|e| anyhow!("failed to materialize tensor as f32: {:?}", e))
}

/// Sample a token from logits using top-k sampling with temperature
///
/// Implements the same algorithm as Python:
/// 1. Apply temperature scaling
/// 2. Top-k filtering
/// 3. Softmax to get probabilities
/// 4. Sample with argmax(probs / Exp(1))
fn sample_token<B: Backend>(logits: &Tensor<B, 2>, temperature: f32, topk: usize) -> Result<i64> {
    use burn::tensor::Distribution;
    use burn::tensor::activation::softmax;

    // Special case: topk=1 is just argmax (deterministic)
    if topk <= 1 {
        return argmax_token(logits);
    }

    // Step 1: Apply temperature scaling
    let scaled = logits.clone() / temperature;

    // Step 2: Top-k filtering - simplified approach
    // Get top-k and use only those for sampling
    let vocab_size = scaled.dims()[1];
    let k = topk.min(vocab_size).max(2); // At least 2 for meaningful sampling

    // Get top-k values and indices in a single call
    let (topk_values, topk_indices) = scaled.clone().topk_with_indices(k, 1);

    // Step 3: Softmax to get probabilities over top-k
    let probs = softmax(topk_values, 1);

    // Step 4: Match Python's _multinomial_sample_one_no_sync:
    // q = exponential_(1); sample = argmax(probs / q)
    let uniform = Tensor::<B, 2>::random([1, k], Distribution::Uniform(0.0, 1.0), &probs.device())
        .cast(burn::tensor::DType::F32);
    let uniform_data = uniform.to_data();
    let uniform_vec: Vec<f32> = uniform_data
        .to_vec()
        .map_err(|e| anyhow!("failed to get uniform random data: {:?}", e))?;
    let probs_data = probs.cast(burn::tensor::DType::F32).to_data();
    let probs_vec: Vec<f32> = probs_data
        .to_vec()
        .map_err(|e| anyhow!("failed to get probability data: {:?}", e))?;

    let mut selected_idx = 0usize;
    let mut best_score = f32::NEG_INFINITY;
    for (i, (&u, &p)) in uniform_vec.iter().zip(probs_vec.iter()).enumerate() {
        let clamped = u.clamp(f32::MIN_POSITIVE, 1.0);
        let q = -clamped.ln();
        let score = p / q;
        if score > best_score {
            best_score = score;
            selected_idx = i;
        }
    }

    // Get the actual token ID from the top-k indices
    let token_data = topk_indices
        .slice([0..1, selected_idx..selected_idx + 1])
        .to_data();
    let token_vec: Vec<i64> = token_data
        .to_vec()
        .map_err(|_| anyhow!("failed to get token"))?;
    let token = token_vec[0];

    Ok(token)
}

/// Legacy argmax token for backward compatibility
fn argmax_token<B: Backend>(logits: &Tensor<B, 2>) -> Result<i64> {
    let logits_data = logits.clone().cast(DType::F32).to_data();
    let logits_vec: Vec<f32> = logits_data
        .to_vec()
        .map_err(|e| anyhow!("failed to get logits for argmax: {:?}", e))?;
    let dims = logits.dims();
    let vocab_size = *dims
        .last()
        .ok_or_else(|| anyhow!("argmax_token expected non-empty logits shape"))?;
    if vocab_size == 0 || logits_vec.is_empty() {
        return Err(anyhow!("argmax_token received empty logits"));
    }
    let row = &logits_vec[..vocab_size];
    let mut best_index = 0usize;
    let mut best_value = f32::NEG_INFINITY;
    for (index, &value) in row.iter().enumerate() {
        if value > best_value {
            best_value = value;
            best_index = index;
        }
    }
    Ok(best_index as i64)
}

fn linear_no_bias<B: Backend>(device: &B::Device, d_input: usize, d_output: usize) -> Linear<B> {
    LinearConfig::new(d_input, d_output)
        .with_bias(false)
        .with_layout(LinearLayout::Col)
        .init(device)
}

fn linear_with_bias<B: Backend>(device: &B::Device, d_input: usize, d_output: usize) -> Linear<B> {
    LinearConfig::new(d_input, d_output)
        .with_layout(LinearLayout::Col)
        .init(device)
}

fn uninitialized_param<B: Backend, const D: usize>(
    shape: [usize; D],
    device: &B::Device,
) -> Param<Tensor<B, D>> {
    Param::uninitialized(
        ParamId::new(),
        move |device, _require_grad| Tensor::<B, D>::zeros(shape, device),
        device.clone(),
        false,
        shape.into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tags_returns_expected() {
        assert_eq!(super::default_tags(), "<tag></tag>");
    }

    #[test]
    fn normalize_tags_adds_wrappers() {
        let input = "pop, electronic";
        let result = super::normalize_tags(input);
        assert_eq!(result, "<tag>pop,electronic</tag>");
    }

    #[test]
    fn normalize_tags_preserves_existing_wrappers() {
        let input = "<tag>pop</tag>";
        let result = super::normalize_tags(input);
        assert_eq!(result, "<tag>pop</tag>");
    }

    #[test]
    fn normalize_tags_removes_spaces_after_commas() {
        let input = "pop, rock, jazz";
        let result = super::normalize_tags(input);
        assert_eq!(result, "<tag>pop,rock,jazz</tag>");
    }

    #[test]
    fn normalize_tags_converts_to_lowercase() {
        let input = "POP, ROCK";
        let result = super::normalize_tags(input);
        assert_eq!(result, "<tag>pop,rock</tag>");
    }

    #[test]
    fn normalize_tags_trims_whitespace() {
        let input = "  pop, rock  ";
        let result = super::normalize_tags(input);
        assert_eq!(result, "<tag>pop,rock</tag>");
    }

    #[test]
    fn build_prompt_history_basic() {
        let text_bos_id = 1_i64;
        let text_eos_id = 2_i64;
        let lyrics_ids = vec![10, 11, 12];
        let tags_ids = vec![20, 21];

        let history = super::build_prompt_history(text_bos_id, text_eos_id, &lyrics_ids, &tags_ids);

        // Tags should be: [BOS, 20, 21, EOS]
        // Empty separator row
        // Lyrics should be: [BOS, 10, 11, 12, EOS]
        // Total: 4 + 1 + 5 = 10 rows
        assert_eq!(history.len(), 10);

        // Check first tag row has BOS
        assert_eq!(history[0][HEARTMULA_AUDIO_CODEBOOKS], text_bos_id);

        // Check empty separator row
        assert!(history[4].iter().all(|&x| x == 0));

        // Check first lyrics row after separator
        assert_eq!(history[5][HEARTMULA_AUDIO_CODEBOOKS], text_bos_id);
    }

    #[test]
    fn normalize_text_ids_adds_bos_and_eos() {
        let text_bos_id = 1_i64;
        let text_eos_id = 2_i64;
        let ids = vec![10, 11, 12];

        let result = super::normalize_text_ids(text_bos_id, text_eos_id, &ids);

        assert_eq!(result[0], text_bos_id);
        assert_eq!(result[result.len() - 1], text_eos_id);
        assert_eq!(result, vec![1, 10, 11, 12, 2]);
    }

    #[test]
    fn normalize_text_ids_preserves_existing_bos_eos() {
        let text_bos_id = 1_i64;
        let text_eos_id = 2_i64;
        let ids = vec![1, 10, 11, 12, 2];

        let result = super::normalize_text_ids(text_bos_id, text_eos_id, &ids);

        assert_eq!(result, vec![1, 10, 11, 12, 2]);
    }

    #[test]
    fn build_audio_history_row() {
        let frame = vec![100, 200, 300, 400, 500, 600, 700, 800];
        let empty_id = 0_i64;

        let row = super::build_audio_history_row(&frame, empty_id);

        // First 8 elements should be the frame tokens
        for i in 0..HEARTMULA_AUDIO_CODEBOOKS {
            assert_eq!(row[i], frame[i] as i64);
        }
        // The text token position should be empty_id
        assert_eq!(row[HEARTMULA_AUDIO_CODEBOOKS], empty_id);
    }

    #[test]
    fn build_audio_history_row_with_short_frame() {
        let frame = vec![100, 200]; // Only 2 tokens
        let empty_id = 999_i64;

        let row = super::build_audio_history_row(&frame, empty_id);

        // First 2 elements should be the frame tokens
        assert_eq!(row[0], 100);
        assert_eq!(row[1], 200);
        // Remaining audio positions should be empty_id
        for i in 2..HEARTMULA_AUDIO_CODEBOOKS {
            assert_eq!(row[i], empty_id);
        }
    }

    #[test]
    fn single_position_tensor() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let tensor = super::single_position_tensor::<NdArray<f32>>(42, &device);

        assert_eq!(tensor.dims(), [1, 1]);
        let data = tensor.to_data().to_vec::<i64>().unwrap();
        assert_eq!(data[0], 42);
    }

    #[test]
    fn position_tensor() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let positions = vec![0, 1, 2, 3, 4];
        let tensor = super::position_tensor::<NdArray<f32>>(positions, &device);

        assert_eq!(tensor.dims(), [1, 5]);
        let data = tensor.to_data().to_vec::<i64>().unwrap();
        assert_eq!(data, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn repeat_kv_heads() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let tensor = Tensor::<NdArray<f32>, 4>::from_data(
            TensorData::new(vec![1.0; 32], [1, 2, 4, 4]),
            &device,
        );

        let repeated = super::repeat_kv_heads(tensor, 2);
        assert_eq!(repeated.dims(), [1, 2, 8, 4]);
    }

    #[test]
    fn repeat_cached_kv_heads() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let tensor = Tensor::<NdArray<f32>, 4>::from_data(
            TensorData::new(vec![1.0; 32], [1, 4, 2, 4]),
            &device,
        );

        let repeated = super::repeat_cached_kv_heads(tensor, 3);
        assert_eq!(repeated.dims(), [1, 12, 2, 4]);
    }

    #[test]
    fn take_last_token() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let tensor = Tensor::<NdArray<f32>, 3>::from_data(
            TensorData::new(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [1, 3, 2]),
            &device,
        );

        let last = super::take_last_token(tensor);
        assert_eq!(last.dims(), [1, 2]);
    }

    #[test]
    fn tensor_to_f32_vec_success() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let tensor = Tensor::<NdArray<f32>, 2>::from_data(
            TensorData::new(vec![1.0, 2.0, 3.0, 4.0], [2, 2]),
            &device,
        );

        let vec = super::tensor_to_f32_vec(tensor).unwrap();
        assert_eq!(vec, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn write_frames_json_creates_valid_json() {
        use std::io::Read;

        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_frames.json");

        let frames: Vec<Vec<i64>> = vec![
            vec![1, 2, 3, 4, 5, 6, 7, 8],
            vec![9, 10, 11, 12, 13, 14, 15, 16],
        ];

        super::write_frames_json(&path, "test lyrics", "test tags", &frames).unwrap();

        // Read and verify the file
        let mut file = std::fs::File::open(&path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();

        assert!(contents.contains("heartmula"));
        assert!(contents.contains("test lyrics"));
        assert!(contents.contains("test tags"));
        assert!(contents.contains("frame_count"));
        assert!(contents.contains("48000"));

        // Clean up
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn heartmula_generation_config_defaults() {
        let lyrics_ids: &[i64] = &[1, 2, 3];
        let tags_ids: &[i64] = &[4, 5];

        let config = HeartmulaGenerationConfig {
            text_bos_id: 1,
            text_eos_id: 2,
            audio_eos_id: 1000,
            empty_id: 0,
            lyrics_ids,
            tags_ids,
            max_audio_frames: 100,
            temperature: 0.8,
            topk: 25,
            cfg_scale: 2.0,
            progress_callback: None,
        };

        assert_eq!(config.temperature, 0.8);
        assert_eq!(config.topk, 25);
        assert_eq!(config.cfg_scale, 2.0);
        assert_eq!(config.max_audio_frames, 100);
    }

    #[test]
    fn splitmix64_produces_deterministic_sequence() {
        let mut state1 = 123456789_u64;
        let mut state2 = 123456789_u64;

        for _ in 0..10 {
            assert_eq!(
                super::splitmix64_next(&mut state1),
                super::splitmix64_next(&mut state2)
            );
        }
    }

    #[test]
    fn uniform01_open_produces_values_in_range() {
        let mut state = 123456789_u64;

        for _ in 0..100 {
            let value = super::uniform01_open(&mut state);
            assert!(value > 0.0);
            assert!(value < 1.0);
        }
    }

    #[test]
    fn generate_decoder_latent_data_deterministic() {
        let data1 = super::generate_decoder_latent_data(42, 100);
        let data2 = super::generate_decoder_latent_data(42, 100);

        assert_eq!(data1, data2);
        assert_eq!(data1.len(), 100);
    }

    #[test]
    fn scaled_theta_produces_expected_length() {
        let head_dim = 64;
        let theta = super::scaled_theta(head_dim);

        // Should have head_dim / 2 elements
        assert_eq!(theta.len(), head_dim / 2);
    }

    #[test]
    fn scaled_theta_frequency_scaling() {
        let theta_64 = super::scaled_theta(64);
        let theta_128 = super::scaled_theta(128);

        // Both should have frequencies in decreasing order
        for i in 1..theta_64.len() {
            assert!(theta_64[i] <= theta_64[i - 1]);
        }

        for i in 1..theta_128.len() {
            assert!(theta_128[i] <= theta_128[i - 1]);
        }
    }

    #[test]
    fn heartmula_rms_norm_forward() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let norm = super::HeartmulaRmsNorm::<NdArray<f32>>::new(&device, 64, 1e-5);

        let input = Tensor::<NdArray<f32>, 3>::ones([1, 4, 64], &device);
        let output = norm.forward(input);

        assert_eq!(output.dims(), [1, 4, 64]);
    }

    #[test]
    fn heartmula_mlp_forward() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let mlp = super::HeartmulaMlp::<NdArray<f32>>::new(&device);

        let input = Tensor::<NdArray<f32>, 3>::ones([1, 4, HEARTMULA_HIDDEN_SIZE], &device);
        let output = mlp.forward(input);

        assert_eq!(output.dims(), [1, 4, HEARTMULA_HIDDEN_SIZE]);
    }

    #[test]
    fn heartmula_attention_new() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let attn = super::HeartmulaAttention::<NdArray<f32>>::new(
            &device,
            HEARTMULA_BACKBONE_HEADS,
            HEARTMULA_BACKBONE_KV_HEADS,
        );

        assert_eq!(attn.meta.0.num_heads, HEARTMULA_BACKBONE_HEADS);
        assert_eq!(attn.meta.0.num_kv_heads, HEARTMULA_BACKBONE_KV_HEADS);
    }

    #[test]
    fn heartmula_transformer_layer_new() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let layer = super::HeartmulaTransformerLayer::<NdArray<f32>>::new(
            &device,
            HEARTMULA_BACKBONE_HEADS,
            HEARTMULA_BACKBONE_KV_HEADS,
        );

        assert_eq!(layer.attn.meta.0.num_heads, HEARTMULA_BACKBONE_HEADS);
    }

    #[test]
    fn heartmula_transformer_new() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let transformer = super::HeartmulaTransformer::<NdArray<f32>>::new(
            &device,
            HEARTMULA_BACKBONE_LAYERS,
            HEARTMULA_BACKBONE_HEADS,
            HEARTMULA_BACKBONE_KV_HEADS,
        );

        assert_eq!(transformer.layers.len(), HEARTMULA_BACKBONE_LAYERS);
    }

    #[test]
    fn heartmula_model_new() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let model = super::HeartmulaModel::<NdArray<f32>>::new(&device, 1000, 1024);

        // Check that model was created with expected dimensions
        assert_eq!(model.audio_head.dims()[0], HEARTMULA_AUDIO_CODEBOOKS - 1);
        assert_eq!(model.audio_head.dims()[1], HEARTMULA_HIDDEN_SIZE);
        assert_eq!(model.audio_head.dims()[2], 1024); // audio_vocab_size
    }

    #[test]
    fn heartmula_model_audio_vocab_size() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let model = super::HeartmulaModel::<NdArray<f32>>::new(&device, 1000, 1024);

        assert_eq!(model.audio_vocab_size(), 1024);
    }

    #[test]
    fn history_tokens_tensor_shape() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let history: Vec<[i64; HEARTMULA_PARALLEL_TOKENS]> = vec![
            [1, 2, 3, 4, 5, 6, 7, 8, 100],
            [9, 10, 11, 12, 13, 14, 15, 16, 101],
        ];

        let tensor = super::history_tokens_tensor::<NdArray<f32>>(&history, &device);
        assert_eq!(tensor.dims(), [1, 2, HEARTMULA_PARALLEL_TOKENS]);
    }

    #[test]
    fn history_mask_tensor_shape() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let history: Vec<[i64; HEARTMULA_PARALLEL_TOKENS]> =
            vec![[1, 2, 3, 4, 5, 6, 7, 8, 100], [0, 0, 0, 0, 0, 0, 0, 0, 101]];

        let tensor = super::history_mask_tensor::<NdArray<f32>>(&history, &device);
        assert_eq!(tensor.dims(), [1, 2, HEARTMULA_PARALLEL_TOKENS]);
    }

    #[test]
    fn causal_mask_shape() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let mask = super::causal_mask::<NdArray<f32>>(5, &device);

        assert_eq!(mask.dims(), [1, 1, 5, 5]);
    }

    #[test]
    fn causal_mask_values() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let mask = super::causal_mask::<NdArray<f32>>(3, &device);
        let data = mask.to_data().to_vec::<bool>().unwrap();

        // Expected (causal) mask for seq_len=3:
        // [false, true,  true ]
        // [false, false, true ]
        // [false, false, false]
        assert_eq!(data[0], false); // [0,0]
        assert_eq!(data[1], true); // [0,1]
        assert_eq!(data[2], true); // [0,2]
        assert_eq!(data[3], false); // [1,0]
        assert_eq!(data[4], false); // [1,1]
        assert_eq!(data[5], true); // [1,2]
        assert_eq!(data[6], false); // [2,0]
        assert_eq!(data[7], false); // [2,1]
        assert_eq!(data[8], false); // [2,2]
    }

    #[test]
    fn apply_scaled_rope_preserves_shape() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let tensor = Tensor::<NdArray<f32>, 4>::ones([1, 4, 8, 64], &device);
        let positions = Tensor::<NdArray<f32>, 2, Int>::from_data(
            TensorData::new(vec![0, 1, 2, 3], [1, 4]),
            &device,
        );

        let rotated = super::apply_scaled_rope(tensor, &positions);
        assert_eq!(rotated.dims(), [1, 4, 8, 64]);
    }

    #[test]
    fn scaled_rope_cache_shape() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let positions = vec![0, 1, 2, 3, 4];
        let cache = super::scaled_rope_cache::<NdArray<f32>>(&device, &positions, 64);

        assert_eq!(cache.dims(), [5, 32, 2]); // [seq_len, head_dim/2, 2]
    }

    #[test]
    fn splice_sequence_token_middle() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let hidden = Tensor::<NdArray<f32>, 3>::from_data(
            TensorData::new(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [1, 3, 2]),
            &device,
        );
        let replacement = Tensor::<NdArray<f32>, 3>::from_data(
            TensorData::new(vec![9.0, 9.0], [1, 1, 2]),
            &device,
        );

        let result = super::splice_sequence_token(hidden, replacement, 1);
        assert_eq!(result.dims(), [1, 3, 2]);
    }

    #[test]
    fn splice_sequence_token_at_start() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let hidden = Tensor::<NdArray<f32>, 3>::from_data(
            TensorData::new(vec![1.0, 2.0, 3.0, 4.0], [1, 2, 2]),
            &device,
        );
        let replacement = Tensor::<NdArray<f32>, 3>::from_data(
            TensorData::new(vec![9.0, 9.0], [1, 1, 2]),
            &device,
        );

        let result = super::splice_sequence_token(hidden, replacement, 0);
        assert_eq!(result.dims(), [1, 2, 2]);
    }

    #[test]
    fn splice_sequence_token_at_end() {
        use burn::backend::ndarray::NdArray;

        let device = <NdArray<f32> as Backend>::Device::default();
        let hidden = Tensor::<NdArray<f32>, 3>::from_data(
            TensorData::new(vec![1.0, 2.0, 3.0, 4.0], [1, 2, 2]),
            &device,
        );
        let replacement = Tensor::<NdArray<f32>, 3>::from_data(
            TensorData::new(vec![9.0, 9.0], [1, 1, 2]),
            &device,
        );

        let result = super::splice_sequence_token(hidden, replacement, 1);
        assert_eq!(result.dims(), [1, 2, 2]);
    }
}
