//! Convert HuggingFace safetensors to burnpack format
//!
//! Usage: convert_safetensors_to_burnpack <model_dir> <output.bpk>
//!
//! The model_dir should contain:
//! - model.safetensors.index.json (tensor index)
//! - model-00001-of-00002.safetensors (shard 1)
//! - model-00002-of-00002.safetensors (shard 2)

use anyhow::{Context, Result, bail};
use burn::module::ParamId;
use burn::tensor::TensorData;
use burn_store::{BurnpackWriter, TensorSnapshot};
use safetensors::SafeTensors;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args_os();
    let _program = args.next();
    let model_dir = PathBuf::from(args.next().context("missing model directory path")?);
    let output = PathBuf::from(args.next().context("missing output .bpk path")?);

    if args.next().is_some() {
        bail!("usage: convert_safetensors_to_burnpack <model_dir> <output.bpk>");
    }

    eprintln!("Converting HeartCodec safetensors to burnpack");
    eprintln!("  Model dir: {}", model_dir.display());
    eprintln!("  Output: {}", output.display());

    // Load tensor index
    let index_path = model_dir.join("model.safetensors.index.json");
    let index_content = fs::read_to_string(&index_path)
        .with_context(|| format!("failed to read {}", index_path.display()))?;
    let index: serde_json::Value =
        serde_json::from_str(&index_content).context("failed to parse tensor index JSON")?;

    let weight_map = index["weight_map"]
        .as_object()
        .context("weight_map not found in index")?;

    eprintln!("  Found {} tensors in index", weight_map.len());

    // Load all safetensor files
    let mut tensors: HashMap<String, (Vec<f32>, Vec<usize>)> = HashMap::new();

    // Get unique shard files
    let mut shard_files: Vec<&str> = weight_map.values().filter_map(|v| v.as_str()).collect();
    shard_files.sort();
    shard_files.dedup();

    for shard_name in shard_files {
        let shard_path = model_dir.join(shard_name);
        eprintln!("  Loading shard: {}", shard_name);

        let buffer = fs::read(&shard_path)
            .with_context(|| format!("failed to read {}", shard_path.display()))?;
        let safetensors = SafeTensors::deserialize(&buffer)
            .with_context(|| format!("failed to deserialize {}", shard_name))?;

        for (tensor_name, view) in safetensors.tensors() {
            let shape: Vec<usize> = view.shape().to_vec();

            // Convert to f32 based on dtype
            let data: Vec<f32> = match view.dtype() {
                safetensors::Dtype::F32 => {
                    let bytes = view.data();
                    let mut data = Vec::with_capacity(bytes.len() / 4);
                    for chunk in bytes.chunks_exact(4) {
                        data.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                    }
                    data
                }
                safetensors::Dtype::F16 => {
                    let bytes = view.data();
                    let mut data = Vec::with_capacity(bytes.len() / 2);
                    for chunk in bytes.chunks_exact(2) {
                        let val = half::f16::from_le_bytes([chunk[0], chunk[1]]);
                        data.push(val.to_f32());
                    }
                    data
                }
                safetensors::Dtype::I64 => {
                    let bytes = view.data();
                    let mut data = Vec::with_capacity(bytes.len() / 8);
                    for chunk in bytes.chunks_exact(8) {
                        let val = i64::from_le_bytes([
                            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6],
                            chunk[7],
                        ]);
                        data.push(val as f32);
                    }
                    data
                }
                dt => {
                    eprintln!("    Warning: skipping {} with dtype {:?}", tensor_name, dt);
                    continue;
                }
            };

            tensors.insert(tensor_name.to_string(), (data, shape));
        }
    }

    eprintln!("  Loaded {} tensors", tensors.len());

    // Create TensorSnapshots from loaded tensors
    let mut snapshots = Vec::new();

    for (name, (data, shape)) in tensors {
        // Parse the name to create path_stack
        // Name format: "flow_matching.vq_embed.layers.0._codebook.embed"
        // -> path_stack: ["flow_matching", "vq_embed", "layers", "0", "_codebook", "embed"]
        // full_path() joins path_stack with "."

        let path_stack: Vec<String> = name.split('.').map(|s| s.to_string()).collect();

        // Use the first part as a simple container type hint
        let container_stack = if !path_stack.is_empty() {
            vec![format!("Struct:{}", path_stack[0])]
        } else {
            vec![]
        };

        let snapshot = TensorSnapshot::from_data(
            TensorData::new(data, shape),
            path_stack,
            container_stack,
            ParamId::new(),
        );
        snapshots.push(snapshot);
    }

    eprintln!("  Created {} tensor snapshots", snapshots.len());

    // Write burnpack file
    BurnpackWriter::new(snapshots)
        .write_to_file(&output)
        .with_context(|| format!("failed to write {}", output.display()))?;

    eprintln!("  Successfully wrote {}", output.display());

    Ok(())
}
