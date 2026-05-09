"""모델 아키텍처 모음. `--arch` 플래그로 train/export/replay 가 선택.

현재 지원:
  - "pilotnet"  : NVIDIA PilotNet 변형 (default). 약 250k 파라미터.
  - "tiny"      : 경량화 변형. 약 50k 파라미터. 추론 더 빠름, 단순한 코스에 적합.

추가 후보 (TODO):
  - MobileNetV3-Small backbone + regression head (~700k)
  - EfficientNet-B0 (~5M)

같은 입력/출력 형식 유지: 입력 (1, C, 66, 200), 출력 (B, 2)=(steering, throttle).
모든 아키텍처가 mean/std buffer 를 자체 포함 → ONNX export 시 정규화 함께 직렬화.
"""

from __future__ import annotations

from typing import Tuple

import torch
import torch.nn as nn


INPUT_H, INPUT_W = 66, 200
OUTPUT_DIM = 2
DEFAULT_MEAN = (0.45, 0.46, 0.43)
DEFAULT_STD = (0.22, 0.22, 0.22)


def _normalization_buffers(stereo: bool, mean: Tuple, std: Tuple):
    """mean/std 를 (1, C, 1, 1) buffer 텐서로. stereo 면 6채널로 복제."""
    if stereo and len(mean) == 3:
        mean = tuple(mean) + tuple(mean)
        std = tuple(std) + tuple(std)
    in_c = len(mean)
    return (
        torch.tensor(mean).view(1, in_c, 1, 1),
        torch.tensor(std).view(1, in_c, 1, 1),
        in_c,
    )


# ---------------------------------------------------------------------------
# PilotNet (NVIDIA) — default
# ---------------------------------------------------------------------------

class PilotNet(nn.Module):
    """NVIDIA PilotNet 변형. 약 250k 파라미터."""

    INPUT_C, INPUT_H, INPUT_W = 3, 66, 200
    INPUT_C_STEREO = 6
    OUTPUT_DIM = 2
    DEFAULT_MEAN = DEFAULT_MEAN
    DEFAULT_STD = DEFAULT_STD

    def __init__(
        self,
        dropout: float = 0.2,
        output_dim: int = OUTPUT_DIM,
        mean: Tuple = DEFAULT_MEAN,
        std: Tuple = DEFAULT_STD,
        stereo: bool = False,
    ):
        super().__init__()
        self.stereo = stereo
        m, s, in_c = _normalization_buffers(stereo, mean, std)
        self.register_buffer("mean", m)
        self.register_buffer("std", s)

        self.conv1 = nn.Conv2d(in_c, 24, kernel_size=5, stride=2)
        self.conv2 = nn.Conv2d(24, 36, kernel_size=5, stride=2)
        self.conv3 = nn.Conv2d(36, 48, kernel_size=5, stride=2)
        self.conv4 = nn.Conv2d(48, 64, kernel_size=3)
        self.conv5 = nn.Conv2d(64, 64, kernel_size=3)
        self.flatten_dim = 64 * 1 * 18
        self.fc1 = nn.Linear(self.flatten_dim, 100)
        self.fc2 = nn.Linear(100, 50)
        self.fc3 = nn.Linear(50, 10)
        self.head = nn.Linear(10, output_dim)
        self.drop = nn.Dropout(dropout)
        self.act = nn.ReLU(inplace=True)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        x = (x - self.mean) / self.std
        x = self.act(self.conv1(x))
        x = self.act(self.conv2(x))
        x = self.act(self.conv3(x))
        x = self.act(self.conv4(x))
        x = self.act(self.conv5(x))
        x = x.flatten(1)
        x = self.drop(self.act(self.fc1(x)))
        x = self.drop(self.act(self.fc2(x)))
        x = self.act(self.fc3(x))
        return self.head(x)


# ---------------------------------------------------------------------------
# TinyPilotNet — 경량화 변형
# ---------------------------------------------------------------------------

class TinyPilotNet(nn.Module):
    """더 작은 PilotNet 변형. 채널 수 ↓, FC 차원 ↓. 약 50k 파라미터.

    PilotNet 과 동일 입력/출력 형식 — 학습/추론 코드 변경 불필요.
    추론 속도가 더 빠르고 over-fitting 위험 낮음. 단순한 코스나 데이터 적은 환경에 적합.
    """

    INPUT_C, INPUT_H, INPUT_W = 3, 66, 200
    INPUT_C_STEREO = 6
    OUTPUT_DIM = 2

    def __init__(
        self,
        dropout: float = 0.2,
        output_dim: int = OUTPUT_DIM,
        mean: Tuple = DEFAULT_MEAN,
        std: Tuple = DEFAULT_STD,
        stereo: bool = False,
    ):
        super().__init__()
        self.stereo = stereo
        m, s, in_c = _normalization_buffers(stereo, mean, std)
        self.register_buffer("mean", m)
        self.register_buffer("std", s)

        self.conv1 = nn.Conv2d(in_c, 16, kernel_size=5, stride=2)
        self.conv2 = nn.Conv2d(16, 24, kernel_size=5, stride=2)
        self.conv3 = nn.Conv2d(24, 32, kernel_size=5, stride=2)
        self.conv4 = nn.Conv2d(32, 48, kernel_size=3)
        # PilotNet 의 conv5 는 생략 (1 x 18 까지 줄어들어 큰 효과 없음)
        # 입력 (66, 200) → c1(31, 98) → c2(14, 47) → c3(5, 22) → c4(3, 20)
        self.flatten_dim = 48 * 3 * 20
        self.fc1 = nn.Linear(self.flatten_dim, 64)
        self.fc2 = nn.Linear(64, 32)
        self.head = nn.Linear(32, output_dim)
        self.drop = nn.Dropout(dropout)
        self.act = nn.ReLU(inplace=True)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        x = (x - self.mean) / self.std
        x = self.act(self.conv1(x))
        x = self.act(self.conv2(x))
        x = self.act(self.conv3(x))
        x = self.act(self.conv4(x))
        x = x.flatten(1)
        x = self.drop(self.act(self.fc1(x)))
        x = self.drop(self.act(self.fc2(x)))
        return self.head(x)


# ---------------------------------------------------------------------------
# 팩토리
# ---------------------------------------------------------------------------

ARCHS = {
    "pilotnet": PilotNet,
    "tiny": TinyPilotNet,
}


def build(arch: str, **kwargs) -> nn.Module:
    if arch not in ARCHS:
        raise ValueError(f"unknown arch '{arch}'. available: {list(ARCHS.keys())}")
    return ARCHS[arch](**kwargs)


def count_params(model: nn.Module) -> int:
    return sum(p.numel() for p in model.parameters() if p.requires_grad)
