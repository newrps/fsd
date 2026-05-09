//! fsd-ml — behavioral cloning model + 학습/추론 라이브러리.
//!
//! 모듈 구성:
//!   - `model`  : PilotNet 변형 (조향 + 스로틀 회귀, dual-camera 옵션).
//!   - `data`   : recordings/manifest.jsonl 로더 + 이미지 전처리.
//!   - `train`  : 학습 루프 (mini-batch SGD / Adam).
//!   - `infer`  : 추론 헬퍼 (단일 프레임 → DriveCommand).

pub mod model;
pub mod data;
pub mod train;
pub mod infer;

// jetson 등 다른 crate 가 fsd_ml 만 의존해도 backend 를 쓸 수 있도록 re-export.
#[cfg(feature = "ndarray")]
pub use burn::backend::NdArray;
#[cfg(feature = "tch-cpu")]
pub use burn::backend::LibTorch;
#[cfg(feature = "tch-cuda")]
pub use burn::backend::LibTorch;

/// 추론용 기본 backend 타입 alias. ndarray feature 기준.
#[cfg(feature = "ndarray")]
pub type DefaultBackend = burn::backend::NdArray<f32>;

/// 입력 이미지 크기 — NVIDIA PilotNet 표준(66x200x3).
pub const INPUT_H: usize = 66;
pub const INPUT_W: usize = 200;
pub const INPUT_C: usize = 3;

/// 출력 차원 — [steering, throttle].
pub const OUTPUT_DIM: usize = 2;
