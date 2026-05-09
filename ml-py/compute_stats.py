"""manifest 의 모든 이미지를 한 번 훑어 채널별 mean/std 계산.

사용:
  python compute_stats.py --manifest ../recordings/run-001/manifest.jsonl --out stats.json

출력 stats.json:
  {"mean": [0.45, 0.46, 0.43], "std": [0.22, 0.23, 0.21]}

학습 시 train.py 가 stats 가 없으면 자동 호출. 정규화 layer 가 모델 안에 들어가므로
ONNX export 후에도 외부 후처리 불필요.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import torch
from PIL import Image
import torchvision.transforms.functional as TF

from pilotnet import PilotNet


def compute(manifest: Path) -> dict:
    base = manifest.parent
    sum_ = torch.zeros(3, dtype=torch.float64)
    sum_sq = torch.zeros(3, dtype=torch.float64)
    n_pixels = 0
    n_samples = 0
    with manifest.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            cam0 = Path(obj["cam0"])
            path = cam0 if cam0.is_absolute() else base / cam0
            try:
                img = Image.open(path).convert("RGB").resize(
                    (PilotNet.INPUT_W, PilotNet.INPUT_H),
                    Image.BILINEAR,
                )
            except OSError:
                continue
            x = TF.to_tensor(img).double()  # (3, H, W) 0..1
            sum_ += x.sum(dim=(1, 2))
            sum_sq += (x * x).sum(dim=(1, 2))
            n_pixels += x.shape[1] * x.shape[2]
            n_samples += 1

    if n_samples == 0:
        raise SystemExit(f"manifest is empty or unreadable: {manifest}")

    mean = (sum_ / n_pixels).tolist()
    var = (sum_sq / n_pixels) - (sum_ / n_pixels) ** 2
    std = var.clamp_min(1e-8).sqrt().tolist()
    return {"mean": mean, "std": std, "n_samples": n_samples}


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--out", type=Path, default=Path("stats.json"))
    args = p.parse_args()

    stats = compute(args.manifest)
    args.out.write_text(json.dumps(stats, indent=2), encoding="utf-8")
    print(f"wrote {args.out}: mean={stats['mean']} std={stats['std']} n={stats['n_samples']}")


if __name__ == "__main__":
    main()
