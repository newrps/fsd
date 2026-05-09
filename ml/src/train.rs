//! 학습 루프. 단순 MSE 회귀로 시작 — 추후 다른 loss(예: Huber) 도 실험 가능.

use anyhow::Result;
use burn::data::dataloader::DataLoaderBuilder;
use burn::data::dataset::transform::PartialDataset;
use burn::data::dataset::Dataset;
use burn::module::{AutodiffModule, Module};
use burn::optim::{AdamConfig, GradientsParams, Optimizer};
use burn::record::{CompactRecorder, Recorder};
use burn::tensor::backend::AutodiffBackend;
use burn::tensor::ElementConversion;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::sync::Arc;

use crate::data::{DrivingBatch, DrivingBatcher, DrivingDataset, StatsJson};
use crate::model::PilotNetConfig;

#[derive(Debug, Clone)]
pub struct TrainConfig {
    pub epochs: usize,
    pub batch_size: usize,
    pub lr: f64,
    pub workers: usize,
    pub val_split: f32,
    pub seed: u64,
}

impl Default for TrainConfig {
    fn default() -> Self {
        Self { epochs: 30, batch_size: 64, lr: 1e-4, workers: 4, val_split: 0.1, seed: 1337 }
    }
}

pub fn train<B: AutodiffBackend>(
    manifest: &Path,
    out_dir: &Path,
    cfg: TrainConfig,
    device: B::Device,
) -> Result<()>
where
    B::IntElem: From<i32>,
{
    std::fs::create_dir_all(out_dir)?;
    let dataset = Arc::new(DrivingDataset::load(manifest)?);
    let n = dataset.len();
    if n == 0 {
        anyhow::bail!("manifest is empty: {:?}", manifest);
    }
    tracing::info!(samples = n, "dataset loaded");

    // ----- stats: manifest 디렉터리의 stats.json 우선, 없으면 자동 계산 -----
    let stats_path = manifest
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("stats.json");
    let stats = if stats_path.exists() {
        let s = std::fs::read_to_string(&stats_path)?;
        let parsed: StatsJson = serde_json::from_str(&s)?;
        tracing::info!(?stats_path, "loaded existing stats");
        crate::model::InputStats { mean: parsed.mean, std: parsed.std }
    } else {
        tracing::info!("computing stats from dataset...");
        let s = dataset.compute_stats();
        let json = StatsJson { mean: s.mean, std: s.std, n_samples: n as u64 };
        std::fs::write(&stats_path, serde_json::to_string_pretty(&json)?)?;
        tracing::info!(?stats_path, "saved stats");
        s
    };
    tracing::info!(mean = ?stats.mean, std = ?stats.std, "input normalization stats");

    // 학습/검증 분리. PartialDataset::new(dataset, start, end) 로 명시 분할.
    let val_n = ((n as f32) * cfg.val_split).max(1.0) as usize;
    let train_n = n - val_n;
    let train_ds = PartialDataset::new(dataset.clone(), 0, train_n);
    let val_ds = PartialDataset::new(dataset, train_n, n);

    // train 만 augmentation, val 은 원본.
    let train_loader = DataLoaderBuilder::new(DrivingBatcher::train())
        .batch_size(cfg.batch_size)
        .shuffle(cfg.seed)
        .num_workers(cfg.workers)
        .build(train_ds);
    let val_loader = DataLoaderBuilder::new(DrivingBatcher::val())
        .batch_size(cfg.batch_size)
        .num_workers(cfg.workers)
        .build(val_ds);

    let mut config = PilotNetConfig::default();
    config.stats = stats;
    let mut model = config.init::<B>(&device);
    let mut optim = AdamConfig::new().init();

    for epoch in 1..=cfg.epochs {
        let pb = ProgressBar::new(train_loader.num_items() as u64);
        pb.set_style(
            ProgressStyle::with_template("epoch {msg:>3} {bar:40.cyan/blue} {pos}/{len} loss={prefix}")
                .unwrap()
                .progress_chars("##-"),
        );
        pb.set_message(format!("{epoch:>3}"));

        let mut loss_sum = 0.0f32;
        let mut n_seen = 0usize;
        for batch in train_loader.iter() {
            let DrivingBatch { images, targets } = batch;
            let pred = model.forward(images);
            let diff = pred - targets;
            let loss = diff.clone().powf_scalar(2.0_f32).mean();
            let loss_val = loss.clone().into_scalar();
            loss_sum += loss_val.elem::<f32>();
            n_seen += 1;

            let grads = loss.backward();
            let grads = GradientsParams::from_grads(grads, &model);
            model = optim.step(cfg.lr, model, grads);

            pb.set_prefix(format!("{:.5}", loss_val.elem::<f32>()));
            pb.inc(1);
        }
        pb.finish_with_message(format!("{epoch:>3} done"));

        let mut val_loss = 0.0f32;
        let mut val_n = 0usize;
        for batch in val_loader.iter() {
            let DrivingBatch { images, targets } = batch;
            let pred = model.valid().forward(images);
            let diff = pred - targets;
            let loss = diff.powf_scalar(2.0_f32).mean();
            val_loss += loss.into_scalar().elem::<f32>();
            val_n += 1;
        }

        tracing::info!(
            epoch,
            train_loss = loss_sum / n_seen.max(1) as f32,
            val_loss = val_loss / val_n.max(1) as f32,
            "epoch done"
        );

        // 매 에폭마다 체크포인트 저장.
        let ckpt = out_dir.join(format!("epoch-{epoch:03}.mpk"));
        CompactRecorder::new()
            .record(model.clone().into_record(), ckpt.clone())
            .map_err(|e| anyhow::anyhow!("record: {:?}", e))?;
        tracing::info!(?ckpt, "saved");
    }
    Ok(())
}
