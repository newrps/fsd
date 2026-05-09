//! 학습 CLI: `cargo run --bin fsd-train -- --manifest <...> --out checkpoints/`

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "fsd-train")]
struct Args {
    /// manifest.jsonl 경로 (recordings/<run>/manifest.jsonl).
    #[arg(long)]
    manifest: PathBuf,

    /// 체크포인트 출력 디렉터리.
    #[arg(long, default_value = "checkpoints")]
    out: PathBuf,

    #[arg(long, default_value_t = 30)]
    epochs: usize,

    #[arg(long, default_value_t = 64)]
    batch_size: usize,

    #[arg(long, default_value_t = 1e-4)]
    lr: f64,

    #[arg(long, default_value_t = 4)]
    workers: usize,

    #[arg(long, default_value_t = 0.1)]
    val_split: f32,

    #[arg(long, default_value_t = 1337)]
    seed: u64,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let cfg = fsd_ml::train::TrainConfig {
        epochs: args.epochs,
        batch_size: args.batch_size,
        lr: args.lr,
        workers: args.workers,
        val_split: args.val_split,
        seed: args.seed,
    };

    #[cfg(feature = "tch-cuda")]
    {
        use burn::backend::{Autodiff, LibTorch};
        use burn::backend::libtorch::LibTorchDevice;
        type B = Autodiff<LibTorch<f32>>;
        let device = LibTorchDevice::Cuda(0);
        return fsd_ml::train::train::<B>(&args.manifest, &args.out, cfg, device);
    }
    #[cfg(all(feature = "tch-cpu", not(feature = "tch-cuda")))]
    {
        use burn::backend::{Autodiff, LibTorch};
        use burn::backend::libtorch::LibTorchDevice;
        type B = Autodiff<LibTorch<f32>>;
        let device = LibTorchDevice::Cpu;
        return fsd_ml::train::train::<B>(&args.manifest, &args.out, cfg, device);
    }
    #[cfg(all(feature = "ndarray", not(any(feature = "tch-cpu", feature = "tch-cuda"))))]
    {
        use burn::backend::{Autodiff, NdArray};
        type B = Autodiff<NdArray<f32>>;
        let device = Default::default();
        return fsd_ml::train::train::<B>(&args.manifest, &args.out, cfg, device);
    }
    #[allow(unreachable_code)]
    {
        anyhow::bail!("no backend feature enabled. compile with one of: ndarray, tch-cpu, tch-cuda");
    }
}
