"""manifest.jsonl 기반 데이터셋. Jetson side `fsd-jetson record` 의 출력을 그대로 읽는다.

manifest.jsonl 의 각 줄 :
{
  "seq": 0,
  "t": "2026-05-08T...Z",
  "steering": -0.12,
  "throttle":  0.45,
  "cam0": "cam0/00000000.jpg",
  "cam1": null
}
"""

from __future__ import annotations

import json
import random
from pathlib import Path
from typing import NamedTuple

import torch
from PIL import Image
from torch.utils.data import Dataset
import torchvision.transforms.functional as TF

from pilotnet import PilotNet


class Sample(NamedTuple):
    cam0: Path
    steering: float
    throttle: float


class Sample2(NamedTuple):
    cam0: Path
    cam1: Path | None
    steering: float
    throttle: float


class DrivingDataset(Dataset):
    """`stereo=True` 면 cam0+cam1 을 6-channel 텐서로 합쳐 반환. cam1 누락 시 cam0 복제."""

    def __init__(self, manifest: Path, stereo: bool = False):
        manifest = Path(manifest)
        self.base = manifest.parent
        self.stereo = stereo
        self.samples: list[Sample2] = []
        with manifest.open("r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                obj = json.loads(line)
                cam1_str = obj.get("cam1")
                self.samples.append(
                    Sample2(
                        cam0=Path(obj["cam0"]),
                        cam1=Path(cam1_str) if cam1_str else None,
                        steering=float(obj.get("steering", 0.0)),
                        throttle=float(obj.get("throttle", 0.0)),
                    )
                )

    def __len__(self) -> int:
        return len(self.samples)

    def _load(self, p: Path) -> torch.Tensor:
        path = p if p.is_absolute() else (self.base / p)
        img = Image.open(path).convert("RGB").resize(
            (PilotNet.INPUT_W, PilotNet.INPUT_H), Image.BILINEAR,
        )
        return TF.to_tensor(img)

    def __getitem__(self, idx: int) -> tuple[torch.Tensor, torch.Tensor]:
        s = self.samples[idx]
        x0 = self._load(s.cam0)
        if self.stereo:
            x1 = self._load(s.cam1) if s.cam1 is not None else x0.clone()
            x = torch.cat([x0, x1], dim=0)  # (6, H, W)
        else:
            x = x0
        y = torch.tensor([s.steering, s.throttle], dtype=torch.float32)
        return x, y


class AugmentingDataset(Dataset):
    """augmentation 정책은 `ml/src/data.rs` 의 `augment_in_place` 와 1:1 동등하게 유지.

    적용 (augment=True 일 때만):
      - 좌/우 hflip + steering 부호 반전 (50%)
      - 밝기 jitter ×[0.8, 1.2]
      - 대비 jitter ×[0.8, 1.2] (텐서 평균 기준)
      - 가로 shift + steering 보정 (recovery, 30%): ±20px 시프트, 시프트당 0.004 의 steering 보정
    throttle 은 영향 없음.
    """

    # recovery augmentation: 한 픽셀 시프트 당 추가될 steering 양.
    # 200px 폭의 PilotNet 입력에서 20px 시프트 → 0.08 (약 8°) steering 보정.
    # 값이 너무 크면 모델이 작은 위치 변화에 과민 반응.
    RECOVERY_STEERING_PER_PX = 0.004
    RECOVERY_MAX_SHIFT_PX = 20

    def __init__(self, base: Dataset, augment: bool):
        self.base = base
        self.augment = augment

    def __len__(self) -> int:
        return len(self.base)

    def __getitem__(self, idx: int) -> tuple[torch.Tensor, torch.Tensor]:
        x, y = self.base[idx]
        if not self.augment:
            return x, y

        # 1) hflip + steering 부호 반전 (50%)
        # stereo (6채널) 면 cam0(0:3) ↔ cam1(3:6) 도 swap — flipped left view ≈ right view.
        if random.random() < 0.5:
            x = torch.flip(x, dims=[2])  # (C, H, W) -> flip W
            if x.shape[0] == 6:
                x = torch.cat([x[3:6], x[0:3]], dim=0)
            y = y.clone()
            y[0] = -y[0]

        # 2) brightness
        b = random.uniform(0.8, 1.2)
        x = (x * b).clamp(0.0, 1.0)

        # 3) contrast (mean 기준)
        mean = x.mean()
        c = random.uniform(0.8, 1.2)
        x = ((x - mean) * c + mean).clamp(0.0, 1.0)

        # 4) recovery: 가로 shift + steering 보정 (30%).
        # +shift 는 차가 왼쪽으로 치우친 시점 → 우회전 보정 필요 → steering +
        if random.random() < 0.3:
            shift = random.randint(-self.RECOVERY_MAX_SHIFT_PX, self.RECOVERY_MAX_SHIFT_PX)
            if shift != 0:
                x = torch.roll(x, shifts=shift, dims=2)
                # 좌측 또는 우측 가장자리 픽셀은 wrap 되어 들어오므로 0 으로 마스크.
                if shift > 0:
                    x[:, :, :shift] = 0
                else:
                    x[:, :, shift:] = 0
                y = y.clone() if not isinstance(y, torch.Tensor) or y.requires_grad else y.clone()
                y[0] = max(-1.0, min(1.0, float(y[0]) + shift * self.RECOVERY_STEERING_PER_PX))

        return x, y
