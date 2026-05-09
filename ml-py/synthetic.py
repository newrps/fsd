"""실 데이터 없이 파이프라인을 검증하기 위한 synthetic recording 생성기.

생성 데이터 패턴:
  - 이미지: 단조 그라디언트 (좌→우 밝기 증가)
  - 그라디언트의 "중심 위치" 가 frame 마다 sin 곡선으로 좌우 이동
  - steering = 그 중심 위치를 -1..+1 범위로 정규화
  - throttle = 0.5 고정

이 데이터로 학습한 모델은 "이미지 중심 어디가 밝은지 → 그쪽으로 조향" 을 배운다.
모델이 정상이면 replay MAE_steering 이 0.1 이하로 떨어져야 한다.

사용:
  python synthetic.py --out ../recordings/synthetic --n 200
"""

from __future__ import annotations

import argparse
import json
import math
from datetime import datetime, timezone
from pathlib import Path

import numpy as np
from PIL import Image


def gen_image(width: int, height: int, center_x: float) -> Image.Image:
    """`center_x` 가 0..1 범위. 그라디언트가 그 위치를 정점으로 한다."""
    cx_px = int(center_x * width)
    xs = np.arange(width)
    # 가우시안 형태의 밝기 분포 (peak at cx_px).
    sigma = width / 6.0
    profile = np.exp(-((xs - cx_px) ** 2) / (2 * sigma ** 2))
    profile = (profile * 255).astype(np.uint8)  # (W,)
    arr = np.tile(profile, (height, 1))  # (H, W)
    rgb = np.stack([arr, arr, arr], axis=-1)  # (H, W, 3)
    return Image.fromarray(rgb, mode="RGB")


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, default=Path("../recordings/synthetic"))
    p.add_argument("--n", type=int, default=200, help="frame 수")
    p.add_argument("--width", type=int, default=320)
    p.add_argument("--height", type=int, default=180)
    p.add_argument("--seed", type=int, default=42)
    p.add_argument("--stereo", action="store_true",
                   help="cam1 을 cam0 에서 가로로 시프트한 형태로 생성 (parallax 시뮬)")
    p.add_argument("--baseline-shift", type=int, default=12,
                   help="--stereo 시 cam1 = cam0 우측 시프트 픽셀 수 (depth ~1.5m 가정)")
    args = p.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    cam0 = args.out / "cam0"
    cam1 = args.out / "cam1"
    cam0.mkdir(exist_ok=True)
    cam1.mkdir(exist_ok=True)

    rng = np.random.default_rng(args.seed)
    manifest_path = args.out / "manifest.jsonl"
    with manifest_path.open("w", encoding="utf-8") as f:
        for seq in range(args.n):
            # center_x 가 sin 곡선으로 0.1 ~ 0.9 사이 이동.
            phase = (seq / args.n) * 4 * math.pi
            center_x = 0.5 + 0.4 * math.sin(phase)
            # 약간의 노이즈 추가.
            center_x = float(np.clip(center_x + rng.normal(0, 0.02), 0.05, 0.95))

            steering = (center_x - 0.5) * 2.0  # -1..+1 정규화
            throttle = 0.5

            img = gen_image(args.width, args.height, center_x)
            cam0_path = cam0 / f"{seq:08d}.jpg"
            cam1_path = cam1 / f"{seq:08d}.jpg"
            img.save(cam0_path, quality=85)
            if args.stereo:
                # cam1 = cam0 의 우측 시프트 (parallax 흉내). 좌측은 0 으로 채움.
                arr = np.asarray(img)
                shifted = np.zeros_like(arr)
                s = args.baseline_shift
                shifted[:, s:] = arr[:, :-s]
                Image.fromarray(shifted).save(cam1_path, quality=85)
            else:
                img.save(cam1_path, quality=85)  # mono: 좌우 동일

            sample = {
                "seq": seq,
                "t": datetime.now(timezone.utc).isoformat(),
                "steering": steering,
                "throttle": throttle,
                "cam0": str(cam0_path.relative_to(args.out)),
                "cam1": str(cam1_path.relative_to(args.out)),
            }
            f.write(json.dumps(sample) + "\n")

    print(f"wrote {args.n} synthetic frames to {args.out}")
    print(f"  manifest: {manifest_path}")
    print(f"  steering range: [{-1:.2f}, {1:.2f}], throttle = 0.5")


if __name__ == "__main__":
    main()
