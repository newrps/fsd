//! 데이터셋 로더 — Jetson 측 logger 가 만든 manifest.jsonl 을 읽어 PyTorch 스타일의
//! Dataset 처럼 동작.
//!
//! Sample 한 개 = (RGB 이미지 (66x200), [steering, throttle]).

use anyhow::{Context, Result};
use burn::data::dataloader::batcher::Batcher;
use burn::data::dataset::Dataset;
use burn::tensor::{backend::Backend, Tensor, TensorData};
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub seq: u64,
    #[serde(default)]
    pub steering: f32,
    #[serde(default)]
    pub throttle: f32,
    pub cam0: PathBuf,
    pub cam1: Option<PathBuf>,
}

pub struct DrivingDataset {
    samples: Vec<Sample>,
    base: PathBuf,
}

/// 데이터셋 stats — Python `compute_stats.py` 와 호환되는 JSON 으로 저장/로드 가능.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatsJson {
    pub mean: [f32; 3],
    pub std: [f32; 3],
    #[serde(default)]
    pub n_samples: u64,
}

impl DrivingDataset {
    pub fn load(manifest: &Path) -> Result<Self> {
        let base = manifest.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        let f = File::open(manifest).with_context(|| format!("open {:?}", manifest))?;
        let mut samples = Vec::new();
        for line in BufReader::new(f).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let s: Sample = serde_json::from_str(&line)?;
            samples.push(s);
        }
        Ok(Self { samples, base })
    }
}

impl Dataset<DrivingItem> for DrivingDataset {
    fn get(&self, index: usize) -> Option<DrivingItem> {
        let s = self.samples.get(index)?;
        let path = if s.cam0.is_absolute() { s.cam0.clone() } else { self.base.join(&s.cam0) };
        let img = image::open(&path).ok()?.to_rgb8();
        let img = image::imageops::resize(&img, crate::INPUT_W as u32, crate::INPUT_H as u32, FilterType::Triangle);
        // [0,1] 정규화. 채널은 (H, W, C) → (C, H, W).
        let mut chw = vec![0.0f32; crate::INPUT_C * crate::INPUT_H * crate::INPUT_W];
        for (y, x, p) in img.enumerate_pixels() {
            for c in 0..3 {
                let idx = c * crate::INPUT_H * crate::INPUT_W
                    + (y as usize) * crate::INPUT_W
                    + (x as usize);
                chw[idx] = p.0[c] as f32 / 255.0;
            }
        }
        Some(DrivingItem {
            image: chw,
            target: [s.steering, s.throttle],
        })
    }

    fn len(&self) -> usize {
        self.samples.len()
    }
}

impl DrivingDataset {
    /// 이 데이터셋을 1패스 훑어 채널별 mean/std 를 계산.
    /// ml-py/compute_stats.py 와 같은 알고리즘.
    pub fn compute_stats(&self) -> crate::model::InputStats {
        use burn::data::dataset::Dataset;
        let mut sum = [0.0f64; 3];
        let mut sum_sq = [0.0f64; 3];
        let mut n_pixels = 0u64;
        let chan_size = crate::INPUT_H * crate::INPUT_W;
        let n = self.len();
        for i in 0..n {
            let Some(item) = self.get(i) else { continue };
            for c in 0..3 {
                let start = c * chan_size;
                for v in &item.image[start..start + chan_size] {
                    let v = *v as f64;
                    sum[c] += v;
                    sum_sq[c] += v * v;
                }
            }
            n_pixels += chan_size as u64;
        }
        if n_pixels == 0 {
            return crate::model::InputStats::default();
        }
        let n = n_pixels as f64;
        let mean = [sum[0] / n, sum[1] / n, sum[2] / n];
        let var = [
            (sum_sq[0] / n - mean[0] * mean[0]).max(1e-8),
            (sum_sq[1] / n - mean[1] * mean[1]).max(1e-8),
            (sum_sq[2] / n - mean[2] * mean[2]).max(1e-8),
        ];
        crate::model::InputStats {
            mean: [mean[0] as f32, mean[1] as f32, mean[2] as f32],
            std: [var[0].sqrt() as f32, var[1].sqrt() as f32, var[2].sqrt() as f32],
        }
    }
}

#[derive(Debug, Clone)]
pub struct DrivingItem {
    pub image: Vec<f32>,    // (C, H, W) flatten
    pub target: [f32; 2],   // steering, throttle
}

/// 데이터 증강 정책. train 시 `augment=true`, val/test 는 `false`.
///
/// 적용 항목 (적용 시):
///   - 좌/우 hflip + steering 부호 반전 (50%)
///   - 밝기 jitter ×[0.8, 1.2]
///   - 대비 jitter ×[0.8, 1.2] (mean 기준)
///   - 가로 shift + steering 보정 (30%, recovery 학습용): ±20 px, 시프트당 0.004 보정
///
/// throttle 은 좌우 flip 영향 받지 않음.
#[derive(Debug, Clone, Copy, Default)]
pub struct DrivingBatcher {
    pub augment: bool,
}

impl DrivingBatcher {
    pub fn train() -> Self { Self { augment: true } }
    pub fn val() -> Self   { Self { augment: false } }
}

#[derive(Debug, Clone)]
pub struct DrivingBatch<B: Backend> {
    pub images: Tensor<B, 4>,
    pub targets: Tensor<B, 2>,
}

impl<B: Backend> Batcher<DrivingItem, DrivingBatch<B>> for DrivingBatcher {
    fn batch(&self, mut items: Vec<DrivingItem>) -> DrivingBatch<B> {
        let device = B::Device::default();
        if self.augment {
            for it in items.iter_mut() {
                augment_in_place(it);
            }
        }
        let n = items.len();
        let mut images_flat = Vec::with_capacity(n * crate::INPUT_C * crate::INPUT_H * crate::INPUT_W);
        let mut targets_flat = Vec::with_capacity(n * 2);
        for it in items {
            images_flat.extend(it.image);
            targets_flat.extend_from_slice(&it.target);
        }
        let images = Tensor::<B, 1>::from_data(
            TensorData::new(images_flat, [n * crate::INPUT_C * crate::INPUT_H * crate::INPUT_W]),
            &device,
        )
        .reshape([n, crate::INPUT_C, crate::INPUT_H, crate::INPUT_W]);
        let targets = Tensor::<B, 1>::from_data(
            TensorData::new(targets_flat, [n * 2]),
            &device,
        )
        .reshape([n, 2]);
        DrivingBatch { images, targets }
    }
}

// ----- augmentation 구현 ---------------------------------------------------

const RECOVERY_STEERING_PER_PX: f32 = 0.004;
const RECOVERY_MAX_SHIFT_PX: i32 = 20;

fn augment_in_place(item: &mut DrivingItem) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // 1) hflip + steering 부호 반전 (50%)
    if rng.gen_bool(0.5) {
        hflip_chw(&mut item.image, crate::INPUT_C, crate::INPUT_H, crate::INPUT_W);
        item.target[0] = -item.target[0];
    }

    // 2) brightness × [0.8, 1.2]
    let b: f32 = rng.gen_range(0.8..1.2);
    for v in item.image.iter_mut() {
        *v = (*v * b).clamp(0.0, 1.0);
    }

    // 3) contrast × [0.8, 1.2] (전체 평균 기준)
    let mean: f32 = item.image.iter().sum::<f32>() / item.image.len() as f32;
    let c: f32 = rng.gen_range(0.8..1.2);
    for v in item.image.iter_mut() {
        *v = ((*v - mean) * c + mean).clamp(0.0, 1.0);
    }

    // 4) recovery shift (30%): 가로로 ±20px 시프트 + 비례한 steering 보정.
    if rng.gen_bool(0.3) {
        let shift: i32 = rng.gen_range(-RECOVERY_MAX_SHIFT_PX..=RECOVERY_MAX_SHIFT_PX);
        if shift != 0 {
            shift_chw(&mut item.image, crate::INPUT_C, crate::INPUT_H, crate::INPUT_W, shift);
            item.target[0] = (item.target[0] + (shift as f32) * RECOVERY_STEERING_PER_PX)
                .clamp(-1.0, 1.0);
        }
    }
}

/// (C, H, W) flatten 텐서를 좌/우 뒤집는다 (W 축 reverse). 채널 별로, 행 별로 reverse.
fn hflip_chw(chw: &mut [f32], c: usize, h: usize, w: usize) {
    debug_assert_eq!(chw.len(), c * h * w);
    for ch in 0..c {
        for y in 0..h {
            let row_start = ch * h * w + y * w;
            chw[row_start..row_start + w].reverse();
        }
    }
}

/// 가로로 `shift_px` 만큼 시프트. 양수 = 우측으로 (좌측에 0 패딩), 음수 = 좌측으로.
fn shift_chw(chw: &mut [f32], c: usize, h: usize, w: usize, shift_px: i32) {
    let s = shift_px.unsigned_abs() as usize;
    if s == 0 || s >= w { return; }
    for ch in 0..c {
        for y in 0..h {
            let row_start = ch * h * w + y * w;
            let row = &mut chw[row_start..row_start + w];
            if shift_px > 0 {
                // 우측으로 → 0..s 가 0 으로 채워지고 기존 0..w-s 는 s..w 로 이동.
                row.copy_within(0..w - s, s);
                row[..s].fill(0.0);
            } else {
                // 좌측으로 → w-s..w 가 0, 기존 s..w 는 0..w-s 로 이동.
                row.copy_within(s..w, 0);
                row[w - s..].fill(0.0);
            }
        }
    }
}
