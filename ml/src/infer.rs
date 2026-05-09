//! 추론 헬퍼 — Jetson 메인 앱이나 CLI 에서 사용.

use anyhow::Result;
use burn::module::Module;
use burn::record::{CompactRecorder, Recorder};
use burn::tensor::{backend::Backend, Tensor, TensorData};
use std::path::Path;

use crate::model::{PilotNet, PilotNetConfig};

pub struct Inference<B: Backend> {
    pub model: PilotNet<B>,
    pub device: B::Device,
}

impl<B: Backend> Inference<B> {
    pub fn load(ckpt: &Path, device: B::Device) -> Result<Self> {
        let record = CompactRecorder::new()
            .load(ckpt.to_path_buf(), &device)
            .map_err(|e| anyhow::anyhow!("load: {:?}", e))?;
        let model = PilotNetConfig::default().init::<B>(&device).load_record(record);
        Ok(Self { model, device })
    }

    /// 0..1 정규화된 (C=3, H=66, W=200) flatten 입력 → (steering, throttle).
    pub fn predict(&self, image_chw: &[f32]) -> (f32, f32) {
        assert_eq!(image_chw.len(), crate::INPUT_C * crate::INPUT_H * crate::INPUT_W);
        let t = Tensor::<B, 1>::from_data(
            TensorData::new(image_chw.to_vec(), [image_chw.len()]),
            &self.device,
        )
        .reshape([1, crate::INPUT_C, crate::INPUT_H, crate::INPUT_W]);
        let out = self.model.forward(t);
        let v: Vec<f32> = out
            .into_data()
            .convert::<f32>()
            .to_vec()
            .expect("tensor to vec");
        let steering = v[0].clamp(-1.0, 1.0);
        let throttle = v[1].clamp(-1.0, 1.0);
        (steering, throttle)
    }
}
