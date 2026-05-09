"""아키텍처별 학습 + 추론 비교.

각 모델을 같은 데이터로 학습 → ONNX export → bench → MAE/latency 표 출력.

사용:
  python compare_archs.py                       # 기본 (synthetic 200, 25 epoch)
  python compare_archs.py --epochs 30 --n 500
"""

from __future__ import annotations

import argparse
import json
import math
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
import torch
from PIL import Image
from torch.utils.data import DataLoader, random_split

import compute_stats
import models
from dataset import AugmentingDataset, DrivingDataset
import synthetic


HERE = Path(__file__).resolve().parent


def gen_synthetic(out: Path, n: int, seed: int = 42) -> None:
    out.mkdir(parents=True, exist_ok=True)
    (out / "cam0").mkdir(exist_ok=True)
    (out / "cam1").mkdir(exist_ok=True)
    rng = np.random.default_rng(seed)
    with (out / "manifest.jsonl").open("w") as f:
        for seq in range(n):
            phase = (seq / n) * 4 * math.pi
            cx = float(np.clip(0.5 + 0.4 * math.sin(phase) + rng.normal(0, 0.02), 0.05, 0.95))
            steering = (cx - 0.5) * 2.0
            img = synthetic.gen_image(320, 180, cx)
            img.save(out / "cam0" / f"{seq:08d}.jpg", quality=85)
            img.save(out / "cam1" / f"{seq:08d}.jpg", quality=85)
            f.write(json.dumps({
                "seq": seq, "t": "2026-01-01T00:00:00Z",
                "steering": steering, "throttle": 0.5,
                "cam0": f"cam0/{seq:08d}.jpg", "cam1": f"cam1/{seq:08d}.jpg",
            }) + "\n")


def train_one(arch: str, manifest: Path, epochs: int, device: str, stats: dict) -> torch.nn.Module:
    torch.manual_seed(1337)
    ds = DrivingDataset(manifest)
    val_n = max(1, len(ds) // 10)
    train_ds, val_ds = random_split(ds, [len(ds) - val_n, val_n],
                                    generator=torch.Generator().manual_seed(1337))
    train_loader = DataLoader(AugmentingDataset(train_ds, augment=True),
                              batch_size=32, shuffle=True, num_workers=0)
    val_loader = DataLoader(AugmentingDataset(val_ds, augment=False),
                            batch_size=32, shuffle=False, num_workers=0)
    model = models.build(arch, mean=tuple(stats["mean"]), std=tuple(stats["std"])).to(device)
    opt = torch.optim.Adam(model.parameters(), lr=1e-4)
    loss_fn = torch.nn.MSELoss()
    for _ in range(epochs):
        model.train()
        for x, y in train_loader:
            x, y = x.to(device), y.to(device)
            loss = loss_fn(model(x), y)
            opt.zero_grad(set_to_none=True); loss.backward(); opt.step()
    return model


def evaluate_mae(model: torch.nn.Module, manifest: Path, device: str) -> tuple[float, float]:
    model.eval()
    ds = DrivingDataset(manifest)
    s_err, t_err, n = 0.0, 0.0, 0
    with torch.no_grad():
        for x, y in DataLoader(ds, batch_size=64, shuffle=False, num_workers=0):
            out = model(x.to(device)).cpu().numpy()
            s_err += float(np.abs(out[:, 0] - y[:, 0].numpy()).sum())
            t_err += float(np.abs(out[:, 1] - y[:, 1].numpy()).sum())
            n += y.shape[0]
    return s_err / n, t_err / n


def export_onnx(model: torch.nn.Module, out: Path) -> None:
    model.eval()
    dummy = torch.zeros(1, 3, models.INPUT_H, models.INPUT_W)
    torch.onnx.export(model.cpu(), dummy, out,
                      input_names=["input"], output_names=["output"],
                      opset_version=17, do_constant_folding=True, verbose=False)


def bench_onnx(path: Path, iters: int = 1000) -> dict:
    sess = ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])
    in_name = sess.get_inputs()[0].name
    out_name = sess.get_outputs()[0].name
    rng = np.random.default_rng(0)
    dummy = rng.random((1, 3, models.INPUT_H, models.INPUT_W), dtype=np.float32)
    # warmup
    for _ in range(10):
        sess.run([out_name], {in_name: dummy})
    lat = []
    for _ in range(iters):
        t0 = time.perf_counter_ns()
        sess.run([out_name], {in_name: dummy})
        lat.append((time.perf_counter_ns() - t0) / 1000.0)
    lat.sort()
    return {
        "mean_us": sum(lat) / len(lat),
        "p50_us": lat[len(lat) // 2],
        "p99_us": lat[int(len(lat) * 0.99)],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--n", type=int, default=200)
    p.add_argument("--epochs", type=int, default=25)
    p.add_argument("--bench-iters", type=int, default=1000)
    args = p.parse_args()

    device = "cuda" if torch.cuda.is_available() else "cpu"
    work = Path(tempfile.mkdtemp(prefix="fsd-cmp-"))
    print(f"work: {work}")
    print(f"device: {device}")
    try:
        rec = work / "recording"
        gen_synthetic(rec, args.n)
        stats = compute_stats.compute(rec / "manifest.jsonl")

        rows = []
        for arch in models.ARCHS:
            print(f"\n=== {arch} 학습 ===")
            t0 = time.perf_counter()
            model = train_one(arch, rec / "manifest.jsonl", args.epochs, device, stats)
            t_train = time.perf_counter() - t0
            mae_s, mae_t = evaluate_mae(model, rec / "manifest.jsonl", device)
            params = models.count_params(model)
            onnx_path = work / f"{arch}.onnx"
            export_onnx(model, onnx_path)
            sz = onnx_path.stat().st_size
            b = bench_onnx(onnx_path, args.bench_iters)
            rows.append({
                "arch": arch, "params": params, "onnx_kb": sz / 1024,
                "train_s": t_train, "mae_s": mae_s, "mae_t": mae_t, **b,
            })

        print("\n" + "=" * 90)
        print(f"{'arch':<10} {'params':>9} {'onnx KB':>9} {'train s':>9} {'MAE_s':>7} {'MAE_t':>7} {'mean μs':>8} {'p50 μs':>8} {'p99 μs':>8}")
        print("-" * 90)
        for r in rows:
            print(f"{r['arch']:<10} {r['params']:>9,} {r['onnx_kb']:>9.1f} "
                  f"{r['train_s']:>9.1f} {r['mae_s']:>7.4f} {r['mae_t']:>7.4f} "
                  f"{r['mean_us']:>8.0f} {r['p50_us']:>8.0f} {r['p99_us']:>8.0f}")
        print("=" * 90)
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
