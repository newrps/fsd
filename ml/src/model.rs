//! PilotNet 변형. 출력은 (steering, throttle) 두 값을 회귀.
//!
//! 구조 (NVIDIA "End-to-End Learning for Self-Driving Cars"):
//!   Input        : (B, 3, 66, 200)
//!   Conv 5x5/2  ↘ 24
//!   Conv 5x5/2  ↘ 36
//!   Conv 5x5/2  ↘ 48
//!   Conv 3x3    ↘ 64
//!   Conv 3x3    ↘ 64
//!   Flatten
//!   FC 100 → FC 50 → FC 10 → FC 2
//!   각 layer 사이 ReLU, FC 사이 Dropout(0.2).

use burn::module::{Module, Param};
use burn::nn::{
    conv::{Conv2d, Conv2dConfig},
    Dropout, DropoutConfig, Linear, LinearConfig, Relu,
};
use burn::tensor::{backend::Backend, Tensor, TensorData};

/// 데이터셋 stats — Python `compute_stats.py` 의 출력과 동일 구조.
#[derive(Debug, Clone)]
pub struct InputStats {
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

impl Default for InputStats {
    /// `pilotnet.py::PilotNet::DEFAULT_MEAN/STD` 와 동일.
    fn default() -> Self {
        Self {
            mean: [0.45, 0.46, 0.43],
            std: [0.22, 0.22, 0.22],
        }
    }
}

#[derive(Module, Debug)]
pub struct PilotNet<B: Backend> {
    /// 입력 정규화 mean (1,3,1,1). 학습 안 됨(파라미터 아님).
    mean: Param<Tensor<B, 4>>,
    /// 입력 정규화 std (1,3,1,1). 학습 안 됨.
    std: Param<Tensor<B, 4>>,
    conv1: Conv2d<B>,
    conv2: Conv2d<B>,
    conv3: Conv2d<B>,
    conv4: Conv2d<B>,
    conv5: Conv2d<B>,
    fc1: Linear<B>,
    fc2: Linear<B>,
    fc3: Linear<B>,
    head: Linear<B>,
    drop: Dropout,
    relu: Relu,
}

#[derive(Debug, Clone)]
pub struct PilotNetConfig {
    pub dropout: f64,
    pub output_dim: usize,
    pub stats: InputStats,
}

impl Default for PilotNetConfig {
    fn default() -> Self {
        Self {
            dropout: 0.2,
            output_dim: crate::OUTPUT_DIM,
            stats: InputStats::default(),
        }
    }
}

impl PilotNetConfig {
    pub fn init<B: Backend>(&self, device: &B::Device) -> PilotNet<B> {
        // Conv 출력 H, W 계산 (입력 66x200):
        //  c1: 5x5 stride 2  -> 31 x 98 ((66-5)/2+1, (200-5)/2+1)
        //  c2: 5x5 stride 2  -> 14 x 47
        //  c3: 5x5 stride 2  ->  5 x 22
        //  c4: 3x3 stride 1  ->  3 x 20
        //  c5: 3x3 stride 1  ->  1 x 18
        // flatten = 64 * 1 * 18 = 1152
        let flat = 64 * 1 * 18;

        let mean = Tensor::<B, 1>::from_data(
            TensorData::new(self.stats.mean.to_vec(), [3]),
            device,
        )
        .reshape([1, 3, 1, 1]);
        let std = Tensor::<B, 1>::from_data(
            TensorData::new(self.stats.std.to_vec(), [3]),
            device,
        )
        .reshape([1, 3, 1, 1]);

        PilotNet {
            mean: Param::from_tensor(mean),
            std: Param::from_tensor(std),
            conv1: Conv2dConfig::new([3, 24], [5, 5]).with_stride([2, 2]).init(device),
            conv2: Conv2dConfig::new([24, 36], [5, 5]).with_stride([2, 2]).init(device),
            conv3: Conv2dConfig::new([36, 48], [5, 5]).with_stride([2, 2]).init(device),
            conv4: Conv2dConfig::new([48, 64], [3, 3]).init(device),
            conv5: Conv2dConfig::new([64, 64], [3, 3]).init(device),
            fc1: LinearConfig::new(flat, 100).init(device),
            fc2: LinearConfig::new(100, 50).init(device),
            fc3: LinearConfig::new(50, 10).init(device),
            head: LinearConfig::new(10, self.output_dim).init(device),
            drop: DropoutConfig::new(self.dropout).init(),
            relu: Relu::new(),
        }
    }
}

impl<B: Backend> PilotNet<B> {
    /// `images` shape = (B, 3, H, W). H/W = (66, 200) 가정.
    pub fn forward(&self, images: Tensor<B, 4>) -> Tensor<B, 2> {
        // 정규화 — 모델 안에 포함되어 ONNX export 시 함께 export 된다.
        let images = (images - self.mean.val()) / self.std.val();
        let x = self.relu.forward(self.conv1.forward(images));
        let x = self.relu.forward(self.conv2.forward(x));
        let x = self.relu.forward(self.conv3.forward(x));
        let x = self.relu.forward(self.conv4.forward(x));
        let x = self.relu.forward(self.conv5.forward(x));

        let dims = x.dims();
        let flat = x.reshape([dims[0], dims[1] * dims[2] * dims[3]]);

        let x = self.relu.forward(self.fc1.forward(flat));
        let x = self.drop.forward(x);
        let x = self.relu.forward(self.fc2.forward(x));
        let x = self.drop.forward(x);
        let x = self.relu.forward(self.fc3.forward(x));
        // 출력은 tanh 로 -1..1 범위에 묶는 것이 안정적이지만,
        // 학습 데이터의 분포에 따라 raw linear 가 더 잘 맞는 경우도 있어 일단 raw.
        self.head.forward(x)
    }
}
