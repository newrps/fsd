//! 단일 이미지로 추론 테스트: `cargo run --bin fsd-infer -- --ckpt ... --image ...`

use anyhow::Result;
use clap::Parser;
use image::imageops::FilterType;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "fsd-infer")]
struct Args {
    #[arg(long)]
    ckpt: PathBuf,

    #[arg(long)]
    image: PathBuf,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let img = image::open(&args.image)?.to_rgb8();
    let img = image::imageops::resize(&img, fsd_ml::INPUT_W as u32, fsd_ml::INPUT_H as u32, FilterType::Triangle);
    let mut chw = vec![0.0f32; fsd_ml::INPUT_C * fsd_ml::INPUT_H * fsd_ml::INPUT_W];
    for (y, x, p) in img.enumerate_pixels() {
        for c in 0..3 {
            let idx = c * fsd_ml::INPUT_H * fsd_ml::INPUT_W
                + (y as usize) * fsd_ml::INPUT_W
                + (x as usize);
            chw[idx] = p.0[c] as f32 / 255.0;
        }
    }

    #[cfg(any(feature = "tch-cpu", feature = "tch-cuda"))]
    {
        use burn::backend::LibTorch;
        use burn::backend::libtorch::LibTorchDevice;
        let device = if cfg!(feature = "tch-cuda") {
            LibTorchDevice::Cuda(0)
        } else {
            LibTorchDevice::Cpu
        };
        let inf = fsd_ml::infer::Inference::<LibTorch<f32>>::load(&args.ckpt, device)?;
        let (s, t) = inf.predict(&chw);
        println!("steering={s:.4} throttle={t:.4}");
        return Ok(());
    }
    #[cfg(all(feature = "ndarray", not(any(feature = "tch-cpu", feature = "tch-cuda"))))]
    {
        use burn::backend::NdArray;
        let inf = fsd_ml::infer::Inference::<NdArray<f32>>::load(&args.ckpt, Default::default())?;
        let (s, t) = inf.predict(&chw);
        println!("steering={s:.4} throttle={t:.4}");
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        anyhow::bail!("no backend feature enabled");
    }
}
