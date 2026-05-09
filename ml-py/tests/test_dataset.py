"""dataset.py + AugmentingDataset 단위 테스트."""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest
import torch

import synthetic
from dataset import AugmentingDataset, DrivingDataset


@pytest.fixture
def tiny_recording(tmp_path: Path) -> Path:
    """3 frame 짜리 mini recording 디렉터리."""
    (tmp_path / "cam0").mkdir()
    (tmp_path / "cam1").mkdir()
    rows = []
    for seq in range(3):
        img = synthetic.gen_image(320, 180, 0.5 + 0.1 * seq)
        cam0_rel = f"cam0/{seq:08d}.jpg"
        cam1_rel = f"cam1/{seq:08d}.jpg"
        img.save(tmp_path / cam0_rel)
        img.save(tmp_path / cam1_rel)
        rows.append({
            "seq": seq, "t": "2026-01-01T00:00:00Z",
            "steering": 0.1 * seq, "throttle": 0.5,
            "cam0": cam0_rel, "cam1": cam1_rel,
        })
    manifest = tmp_path / "manifest.jsonl"
    with manifest.open("w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")
    return manifest


def test_dataset_loads_3_samples(tiny_recording: Path):
    ds = DrivingDataset(tiny_recording)
    assert len(ds) == 3
    x, y = ds[0]
    assert x.shape == (3, 66, 200)
    assert y.shape == (2,)
    assert 0 <= x.min() and x.max() <= 1


def test_dataset_stereo_returns_6ch(tiny_recording: Path):
    ds = DrivingDataset(tiny_recording, stereo=True)
    x, _ = ds[0]
    assert x.shape == (6, 66, 200), "stereo 모드는 6채널 (cam0+cam1)"


def test_aug_passes_through_when_off(tiny_recording: Path):
    base = DrivingDataset(tiny_recording)
    aug = AugmentingDataset(base, augment=False)
    x_b, y_b = base[1]
    x_a, y_a = aug[1]
    assert torch.allclose(x_b, x_a)
    assert torch.allclose(y_b, y_a)


def test_aug_changes_value_when_on(tiny_recording: Path):
    """augment=True 면 결정적이지 않으므로, 여러 번 뽑으면 다른 값이 나와야 함."""
    base = DrivingDataset(tiny_recording)
    aug = AugmentingDataset(base, augment=True)
    samples = [aug[1] for _ in range(20)]
    xs = [s[0].mean().item() for s in samples]
    assert max(xs) - min(xs) > 0.001, "augment 가 실제로 영향 미쳐야 함 (밝기/대비 jitter)"


def test_aug_steering_inverted_on_hflip(tiny_recording: Path):
    """hflip 이 일어나면 steering 부호가 반전됐음을 확인 (부호 변화 분포)."""
    base = DrivingDataset(tiny_recording)
    aug = AugmentingDataset(base, augment=True)
    # seq=2 의 base steering 0.2. hflip 시 -0.2 근방. 50% 확률 → 시도 횟수 늘려 둘 다 관찰.
    sees_negative = False
    sees_positive = False
    for _ in range(50):
        _, y = aug[2]
        if y[0] < 0:
            sees_negative = True
        elif y[0] > 0:
            sees_positive = True
    assert sees_negative and sees_positive, "hflip 50% 적용 시 steering 양/음 둘 다 나와야 함"
