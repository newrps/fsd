//! 추론 백엔드 추상화. 모델 파일 확장자를 보고 적절한 백엔드를 자동 선택한다.
//!
//! 지원 매트릭스 (feature 별):
//!   `--features onnx`           : .onnx 파일을 ort(CPU) 로 추론
//!   `--features onnx-cuda`      : .onnx 파일을 ort + CUDA EP 로 추론
//!   `--features onnx-tensorrt`  : .onnx 파일을 ort + TensorRT EP 로 추론 (Jetson 권장)
//!   `--features burn-inference` : .mpk 파일을 burn 으로 직접 추론
//!
//! 입력 텐서 형식: (1, 3, 66, 200), float32, 0..1 정규화. CHW.
//! 출력 텐서 형식: (1, 2)  → [steering, throttle] 모두 -1..1 로 clamp.

use anyhow::{bail, Result};
use std::path::Path;

pub trait Predictor: Send {
    /// `image_chw` 길이 = 3 * 66 * 200 = 39600.
    fn predict(&mut self, image_chw: &[f32]) -> Result<(f32, f32)>;
}

#[allow(unreachable_code, unused_variables)]
pub fn load(path: &Path) -> Result<Box<dyn Predictor>> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    #[cfg(feature = "onnx")]
    if ext == "onnx" {
        return Ok(Box::new(onnx_impl::OrtPredictor::new(path)?));
    }
    #[cfg(feature = "burn-inference")]
    if ext == "mpk" {
        return Ok(Box::new(burn_impl::BurnPredictor::new(path)?));
    }
    let mut hints = Vec::new();
    if cfg!(not(feature = "onnx"))           { hints.push("onnx"); }
    if cfg!(not(feature = "burn-inference")) { hints.push("burn-inference"); }
    bail!(
        "확장자 '{ext}' 미지원 또는 해당 백엔드 비활성 ({:?}). \
         cargo build --features {} 로 활성화하세요.",
        path,
        hints.join(",")
    )
}

// ---------------------------------------------------------------------------
// ORT (ONNX Runtime) 백엔드
// ---------------------------------------------------------------------------

#[cfg(feature = "onnx")]
mod onnx_impl {
    use super::Predictor;
    use anyhow::{anyhow, Context, Result};
    use ort::session::{builder::GraphOptimizationLevel, Session};
    use ort::value::Tensor;
    use std::path::Path;

    pub struct OrtPredictor {
        session: Session,
        input_name: String,
        output_name: String,
    }

    /// ort 2.0-rc 의 typed-error 를 anyhow 로 통일하기 위한 작은 헬퍼.
    fn into_any<T, E: std::fmt::Display>(r: std::result::Result<T, E>) -> Result<T> {
        r.map_err(|e| anyhow!("ort: {e}"))
    }

    impl OrtPredictor {
        pub fn new(path: &Path) -> Result<Self> {
            #[allow(unused_mut)]
            let mut builder = into_any(Session::builder())?;
            builder = into_any(builder.with_optimization_level(GraphOptimizationLevel::Level3))?;

            // EP 우선순위: TensorRT > CUDA > CPU. feature 로 활성화된 것만 시도.
            #[cfg(feature = "onnx-tensorrt")]
            {
                use ort::execution_providers::TensorRTExecutionProvider;
                builder = into_any(builder.with_execution_providers([
                    TensorRTExecutionProvider::default().with_fp16(true).build(),
                ]))?;
            }
            #[cfg(all(feature = "onnx-cuda", not(feature = "onnx-tensorrt")))]
            {
                use ort::execution_providers::CUDAExecutionProvider;
                builder = into_any(builder.with_execution_providers([
                    CUDAExecutionProvider::default().build(),
                ]))?;
            }

            let session = into_any(builder.commit_from_file(path))
                .with_context(|| format!("ort load {:?}", path))?;

            let input_name = session
                .inputs()
                .first()
                .ok_or_else(|| anyhow!("model has no inputs"))?
                .name()
                .to_string();
            let output_name = session
                .outputs()
                .first()
                .ok_or_else(|| anyhow!("model has no outputs"))?
                .name()
                .to_string();
            tracing::info!(?input_name, ?output_name, "onnx model loaded");

            Ok(Self { session, input_name, output_name })
        }
    }

    impl Predictor for OrtPredictor {
        fn predict(&mut self, image_chw: &[f32]) -> Result<(f32, f32)> {
            assert_eq!(image_chw.len(), 3 * 66 * 200);
            // ort 2.0 의 from_array 는 (shape, Vec) 튜플을 받는 변형이 있다 — ndarray 의존 X.
            let input_value = into_any(
                Tensor::from_array(([1usize, 3, 66, 200], image_chw.to_vec()))
            )?;
            let outputs = into_any(self
                .session
                .run(ort::inputs![self.input_name.as_str() => input_value]))?;
            let (_shape, data) = into_any(
                outputs[self.output_name.as_str()].try_extract_tensor::<f32>()
            )?;
            if data.len() < 2 {
                return Err(anyhow!("output dim < 2: got {}", data.len()));
            }
            Ok((data[0].clamp(-1.0, 1.0), data[1].clamp(-1.0, 1.0)))
        }
    }
}

// ---------------------------------------------------------------------------
// burn 백엔드
// ---------------------------------------------------------------------------

#[cfg(feature = "burn-inference")]
mod burn_impl {
    use super::Predictor;
    use anyhow::Result;
    use std::path::Path;

    pub struct BurnPredictor {
        inner: fsd_ml::infer::Inference<fsd_ml::DefaultBackend>,
    }

    impl BurnPredictor {
        pub fn new(path: &Path) -> Result<Self> {
            let inner = fsd_ml::infer::Inference::<fsd_ml::DefaultBackend>::load(
                path,
                Default::default(),
            )?;
            tracing::info!(?path, "burn model loaded");
            Ok(Self { inner })
        }
    }

    impl Predictor for BurnPredictor {
        fn predict(&mut self, image_chw: &[f32]) -> Result<(f32, f32)> {
            Ok(self.inner.predict(image_chw))
        }
    }
}
