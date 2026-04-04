use burn::tensor::DType;
use burn_store::{BurnpackStore, ModuleStore};
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().map(std::path::PathBuf::from).unwrap_or_else(|| {
        std::path::PathBuf::from(
            "/home/meka/repos/heartmula-burn/artifacts/heartmula-happy-new-year-20260123/burn_raw/heartmula_raw_f32.bpk",
        )
    });
    let names: Vec<String> = args.collect();
    let start = Instant::now();
    let mut store = BurnpackStore::from_file(&path).zero_copy(true);
    println!("Store opened in {:?}", start.elapsed());
    let start = Instant::now();
    let snapshots = store.get_all_snapshots().unwrap();
    println!("Snapshots loaded in {:?}", start.elapsed());
    println!("Tensor count: {}", snapshots.len());

    if names.is_empty() {
        return;
    }

    for name in names {
        let Some((_, snapshot)) = snapshots.iter().find(|(_, snap)| snap.full_path() == name)
        else {
            println!("{name}: missing");
            continue;
        };

        let data = snapshot.to_data().unwrap();
        match data.dtype {
            DType::F32 => {
                let values = data.to_vec::<f32>().unwrap();
                let take = values.len().min(10);
                let prefix_sum: f32 = values.iter().take(take).copied().sum();
                let l2: f64 = values
                    .iter()
                    .map(|v| {
                        let v = *v as f64;
                        v * v
                    })
                    .sum::<f64>()
                    .sqrt();
                println!(
                    "{name}: shape={:?} dtype={:?} prefix_sum_10={} l2={}",
                    data.shape, data.dtype, prefix_sum, l2
                );
            }
            _ => {
                println!("{name}: shape={:?} dtype={:?}", data.shape, data.dtype);
            }
        }
    }
}
