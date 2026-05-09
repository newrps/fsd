"""PilotNet (NVIDIA End-to-End Learning for Self-Driving Cars).

`ml/`(Rust burn) 의 PilotNet 과 동일한 구조로 유지. 입력/출력 텐서 모양이 같아야
양 경로의 ONNX 가 jetson 측에서 동일하게 동작한다.

  Input  : (B, 3, 66, 200) — RGB, 0..1 정규화
  Output : (B, 2)          — [steering, throttle], -1..1 (학습 데이터 분포에 따름)
"""

from __future__ import annotations

import torch
import torch.nn as nn


class PilotNet(nn.Module):
    INPUT_C, INPUT_H, INPUT_W = 3, 66, 200      # mono (cam0 만)
    INPUT_C_STEREO = 6                          # stereo (cam0 + cam1, 6-channel)
    OUTPUT_DIM = 2

    # 데이터셋 stats 가 없을 때 사용하는 기본값 — 도로 영상에 대한 보수적 추정.
    DEFAULT_MEAN = (0.45, 0.46, 0.43)
    DEFAULT_STD = (0.22, 0.22, 0.22)

    def __init__(
        self,
        dropout: float = 0.2,
        output_dim: int = OUTPUT_DIM,
        mean: tuple = DEFAULT_MEAN,
        std: tuple = DEFAULT_STD,
        stereo: bool = False,
    ):
        super().__init__()
        self.stereo = stereo
        in_channels = self.INPUT_C_STEREO if stereo else self.INPUT_C
        # stereo 면 mean/std 도 6채널로 (cam0/cam1 동일 센서 가정).
        if stereo and len(mean) == 3:
            mean = tuple(mean) + tuple(mean)
            std = tuple(std) + tuple(std)
        # mean/std 는 buffer 로 등록 → state_dict 에 저장 + ONNX export 에 포함.
        self.register_buffer("mean", torch.tensor(mean).view(1, in_channels, 1, 1))
        self.register_buffer("std", torch.tensor(std).view(1, in_channels, 1, 1))
        self.conv1 = nn.Conv2d(in_channels, 24, kernel_size=5, stride=2)
        self.conv2 = nn.Conv2d(24, 36, kernel_size=5, stride=2)
        self.conv3 = nn.Conv2d(36, 48, kernel_size=5, stride=2)
        self.conv4 = nn.Conv2d(48, 64, kernel_size=3)
        self.conv5 = nn.Conv2d(64, 64, kernel_size=3)
        # 입력 66x200 → c1(31x98) → c2(14x47) → c3(5x22) → c4(3x20) → c5(1x18)
        self.flatten_dim = 64 * 1 * 18
        self.fc1 = nn.Linear(self.flatten_dim, 100)
        self.fc2 = nn.Linear(100, 50)
        self.fc3 = nn.Linear(50, 10)
        self.head = nn.Linear(10, output_dim)
        self.drop = nn.Dropout(dropout)
        self.act = nn.ReLU(inplace=True)

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        # 정규화 (buffer 로 들어가 ONNX 안에 포함됨).
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
