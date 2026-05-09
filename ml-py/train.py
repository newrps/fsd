"""Behavioral cloning 학습 스크립트.

사용:
  python train.py --manifest ../recordings/run-001/manifest.jsonl \
                  --out checkpoints --epochs 30 --batch-size 64
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

import torch
import torch.nn as nn
from torch.utils.data import DataLoader, random_split
from tqdm import tqdm

import compute_stats
import models
from dataset import AugmentingDataset, DrivingDataset
from pilotnet import PilotNet  # backward compat


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--out", type=Path, default=Path("checkpoints"))
    p.add_argument("--epochs", type=int, default=30)
    p.add_argument("--batch-size", type=int, default=64)
    p.add_argument("--lr", type=float, default=1e-4)
    p.add_argument("--workers", type=int, default=4)
    p.add_argument("--val-split", type=float, default=0.1)
    p.add_argument("--seed", type=int, default=1337)
    p.add_argument("--stereo", action="store_true",
                   help="cam0+cam1 을 6채널 입력으로 학습 (multi-camera fusion)")
    p.add_argument("--arch", default="pilotnet", choices=list(models.ARCHS.keys()),
                   help="모델 아키텍처. 기본 pilotnet (~250k), tiny (~50k, 더 빠름)")
    args = p.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    torch.manual_seed(args.seed)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    print(f"device: {device}")

    # stats: manifest 디렉터리의 stats.json 우선, 없으면 자동 계산.
    stats_path = args.manifest.parent / "stats.json"
    if stats_path.exists():
        stats = json.loads(stats_path.read_text())
        print(f"loaded stats: {stats_path}")
    else:
        print(f"computing stats from {args.manifest}...")
        stats = compute_stats.compute(args.manifest)
        stats_path.write_text(json.dumps(stats, indent=2))
        print(f"saved stats: {stats_path}")
    print(f"  mean={stats['mean']} std={stats['std']}")

    ds = DrivingDataset(args.manifest, stereo=args.stereo)
    if args.stereo:
        print("stereo mode: 6-channel input (cam0+cam1)")
    n = len(ds)
    if n == 0:
        raise SystemExit(f"empty manifest: {args.manifest}")
    val_n = max(1, int(n * args.val_split))
    train_n = n - val_n
    train_ds, val_ds = random_split(
        ds, [train_n, val_n], generator=torch.Generator().manual_seed(args.seed)
    )
    print(f"samples: train={train_n} val={val_n}")

    # train 만 augmentation, val 은 원본.
    train_aug = AugmentingDataset(train_ds, augment=True)
    val_clean = AugmentingDataset(val_ds, augment=False)

    train_loader = DataLoader(
        train_aug, batch_size=args.batch_size, shuffle=True,
        num_workers=args.workers, pin_memory=(device == "cuda"),
    )
    val_loader = DataLoader(
        val_clean, batch_size=args.batch_size, shuffle=False,
        num_workers=args.workers, pin_memory=(device == "cuda"),
    )

    model = models.build(args.arch,
                         mean=tuple(stats["mean"]),
                         std=tuple(stats["std"]),
                         stereo=args.stereo).to(device)
    print(f"arch={args.arch}  params={models.count_params(model):,}")
    opt = torch.optim.Adam(model.parameters(), lr=args.lr)
    loss_fn = nn.MSELoss()

    best_val = math.inf
    for epoch in range(1, args.epochs + 1):
        model.train()
        train_loss = 0.0
        n_seen = 0
        for x, y in tqdm(train_loader, desc=f"epoch {epoch:>3}"):
            x = x.to(device, non_blocking=True)
            y = y.to(device, non_blocking=True)
            pred = model(x)
            loss = loss_fn(pred, y)
            opt.zero_grad(set_to_none=True)
            loss.backward()
            opt.step()
            train_loss += loss.item() * x.size(0)
            n_seen += x.size(0)
        train_loss /= max(n_seen, 1)

        model.eval()
        val_loss = 0.0
        v_seen = 0
        with torch.no_grad():
            for x, y in val_loader:
                x = x.to(device, non_blocking=True)
                y = y.to(device, non_blocking=True)
                val_loss += loss_fn(model(x), y).item() * x.size(0)
                v_seen += x.size(0)
        val_loss /= max(v_seen, 1)

        ckpt_path = args.out / f"epoch-{epoch:03d}.pt"
        torch.save({"model": model.state_dict(), "epoch": epoch}, ckpt_path)
        if val_loss < best_val:
            best_val = val_loss
            torch.save({"model": model.state_dict(), "epoch": epoch}, args.out / "best.pt")
            print(f"epoch {epoch:>3}  train={train_loss:.5f}  val={val_loss:.5f}  *best*")
        else:
            print(f"epoch {epoch:>3}  train={train_loss:.5f}  val={val_loss:.5f}")


if __name__ == "__main__":
    main()
