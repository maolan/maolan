use anyhow::{Context, Result, bail};
use burn::tensor::DType;
use burn_store::{BurnpackStore, BurnpackWriter, ModuleStore, TensorSnapshot};
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = env::args_os();
    let _program = args.next();
    let input = PathBuf::from(args.next().context("missing input .bpk path")?);
    let output = PathBuf::from(args.next().context("missing output .bpk path")?);

    if args.next().is_some() {
        bail!("usage: convert_burnpack <input.bpk> <output.bpk>");
    }

    let mut store = BurnpackStore::from_file(&input);
    let snapshots = store
        .get_all_snapshots()
        .context("failed to read burnpack snapshots")?;

    let mut converted = Vec::with_capacity(snapshots.len());
    let mut converted_count = 0usize;

    for snapshot in snapshots.values() {
        let data = snapshot
            .to_data()
            .with_context(|| format!("failed to materialize snapshot {}", snapshot.full_path()))?;

        let converted_snapshot = if data.dtype == DType::F16 {
            let values = data
                .to_vec::<half::f16>()
                .with_context(|| format!("failed to decode F16 tensor {}", snapshot.full_path()))?
                .into_iter()
                .map(f32::from)
                .collect::<Vec<_>>();
            converted_count += 1;
            TensorSnapshot::from_data(
                burn::tensor::TensorData::new(values, data.shape.clone()),
                snapshot.path_stack.clone().unwrap_or_default(),
                snapshot.container_stack.clone().unwrap_or_default(),
                snapshot
                    .tensor_id
                    .context("tensor snapshot is missing param id")?,
            )
        } else {
            snapshot.clone()
        };

        converted.push(converted_snapshot);
    }

    BurnpackWriter::new(converted)
        .write_to_file(&output)
        .with_context(|| format!("failed to write {}", output.display()))?;

    eprintln!(
        "converted {} F16 tensors from {} to {}",
        converted_count,
        input.display(),
        output.display()
    );

    Ok(())
}
