"""학습 파이프라인 시각화 데모.

회의/리뷰/실차 시연 전 sanity 체크 용도. ipynb 형태가 아닌 평범한 .py 로 작성하지만
`# %%` 로 셀 분리되어 있어 VSCode/Spyder/Jupytext 에서 셀 단위로 실행 가능.

생성물:
  ../recordings/demo/sample_frame.png        — 합성 frame 한 장
  ../recordings/demo/loss_curves.png         — train/val loss
  ../recordings/demo/predictions.png         — 예측 vs 실측 시계열
  ../recordings/demo/sample_predictions.png  — 9개 frame + 예측 라벨

사용:
  python notebook_demo.py
"""

# %% [markdown]
# # fsd ML 파이프라인 시각화 데모
#
# 합성 데이터 생성 → 학습 → 예측 → 시각화. 결과 PNG 들이 `recordings/demo/` 에 저장됨.

# %%
from __future__ import annotations

import json
import math
import shutil
from pathlib import Path

import matplotlib
import matplotlib.pyplot as plt
import numpy as np
import torch

# Windows/Linux 어디서든 한글 표시 가능하도록 fallback chain.
for font_candidate in ("Malgun Gothic", "AppleGothic", "Noto Sans CJK KR", "DejaVu Sans"):
    try:
        matplotlib.rcParams["font.family"] = font_candidate
        matplotlib.rcParams["axes.unicode_minus"] = False
        break
    except Exception:
        continue
from PIL import Image
from torch.utils.data import DataLoader, random_split

import compute_stats
import synthetic
from dataset import AugmentingDataset, DrivingDataset
from pilotnet import PilotNet


# %% [markdown]
# ## 1. 합성 데이터 생성

# %%
HERE = Path(__file__).resolve().parent
OUT = HERE.parent / "recordings" / "demo"
if OUT.exists():
    shutil.rmtree(OUT)
OUT.mkdir(parents=True)

N_FRAMES = 300

import argparse as _ap
synthetic_args = _ap.Namespace(
    out=OUT, n=N_FRAMES, width=320, height=180,
    seed=42, stereo=False, baseline_shift=12,
)
# synthetic.main 은 argparse 직접 사용해서 다시 실행하기 곤란 — 함수 단위로 호출.
rng = np.random.default_rng(synthetic_args.seed)
(OUT / "cam0").mkdir(exist_ok=True)
(OUT / "cam1").mkdir(exist_ok=True)
with (OUT / "manifest.jsonl").open("w") as f:
    for seq in range(N_FRAMES):
        phase = (seq / N_FRAMES) * 4 * math.pi
        center_x = 0.5 + 0.4 * math.sin(phase) + rng.normal(0, 0.02)
        center_x = float(np.clip(center_x, 0.05, 0.95))
        steering = (center_x - 0.5) * 2.0
        img = synthetic.gen_image(synthetic_args.width, synthetic_args.height, center_x)
        cam0_path = OUT / "cam0" / f"{seq:08d}.jpg"
        img.save(cam0_path, quality=85)
        img.save(OUT / "cam1" / f"{seq:08d}.jpg", quality=85)
        f.write(json.dumps({
            "seq": seq, "t": "2026-01-01T00:00:00Z",
            "steering": steering, "throttle": 0.5,
            "cam0": f"cam0/{seq:08d}.jpg", "cam1": f"cam1/{seq:08d}.jpg",
        }) + "\n")
print(f"생성 완료: {N_FRAMES} frames → {OUT}")

# %% [markdown]
# ## 2. 샘플 frame 시각화

# %%
fig, axes = plt.subplots(1, 5, figsize=(15, 3))
sample_seqs = [0, N_FRAMES // 4, N_FRAMES // 2, 3 * N_FRAMES // 4, N_FRAMES - 1]
for ax, seq in zip(axes, sample_seqs):
    img = Image.open(OUT / "cam0" / f"{seq:08d}.jpg")
    ax.imshow(img, cmap="gray")
    ax.set_title(f"seq={seq}\nsteering={(0.4 * math.sin(seq / N_FRAMES * 4 * math.pi) * 2):+.2f}")
    ax.axis("off")
plt.suptitle("synthetic frames — 밝은 띠 위치 ↔ steering")
plt.tight_layout()
plt.savefig(OUT / "sample_frame.png", dpi=120)
plt.close()
print(f"wrote {OUT / 'sample_frame.png'}")

# %% [markdown]
# ## 3. 학습 (간단 — 25 epoch)

# %%
device = "cuda" if torch.cuda.is_available() else "cpu"
torch.manual_seed(1337)

stats = compute_stats.compute(OUT / "manifest.jsonl")
ds = DrivingDataset(OUT / "manifest.jsonl")
val_n = max(1, int(len(ds) * 0.1))
train_n = len(ds) - val_n
train_ds, val_ds = random_split(
    ds, [train_n, val_n], generator=torch.Generator().manual_seed(1337)
)
train_loader = DataLoader(AugmentingDataset(train_ds, augment=True),
                          batch_size=32, shuffle=True, num_workers=0)
val_loader = DataLoader(AugmentingDataset(val_ds, augment=False),
                        batch_size=32, shuffle=False, num_workers=0)

model = PilotNet(mean=tuple(stats["mean"]), std=tuple(stats["std"])).to(device)
opt = torch.optim.Adam(model.parameters(), lr=1e-4)
loss_fn = torch.nn.MSELoss()

EPOCHS = 25
train_losses = []
val_losses = []
for epoch in range(1, EPOCHS + 1):
    model.train()
    tl_sum, tl_n = 0.0, 0
    for x, y in train_loader:
        x, y = x.to(device), y.to(device)
        loss = loss_fn(model(x), y)
        opt.zero_grad(set_to_none=True); loss.backward(); opt.step()
        tl_sum += loss.item() * x.size(0); tl_n += x.size(0)
    model.eval()
    vl_sum, vl_n = 0.0, 0
    with torch.no_grad():
        for x, y in val_loader:
            x, y = x.to(device), y.to(device)
            vl_sum += loss_fn(model(x), y).item() * x.size(0); vl_n += x.size(0)
    train_losses.append(tl_sum / tl_n)
    val_losses.append(vl_sum / vl_n)
    if epoch % 5 == 0 or epoch == 1:
        print(f"epoch {epoch:>3}  train={train_losses[-1]:.4f}  val={val_losses[-1]:.4f}")

# %% [markdown]
# ## 4. 손실 곡선

# %%
fig, ax = plt.subplots(figsize=(8, 5))
ax.plot(range(1, EPOCHS + 1), train_losses, label="train", linewidth=2)
ax.plot(range(1, EPOCHS + 1), val_losses, label="val", linewidth=2)
ax.set_xlabel("epoch"); ax.set_ylabel("MSE loss")
ax.set_title(f"학습 곡선 — final val={val_losses[-1]:.4f}")
ax.set_yscale("log")
ax.legend(); ax.grid(alpha=0.3)
plt.tight_layout()
plt.savefig(OUT / "loss_curves.png", dpi=120)
plt.close()
print(f"wrote {OUT / 'loss_curves.png'}")

# %% [markdown]
# ## 5. 예측 vs 실측 시계열

# %%
model.eval()
all_pred = []
all_actual = []
with torch.no_grad():
    for x, y in DataLoader(ds, batch_size=32, shuffle=False, num_workers=0):
        out = model(x.to(device)).cpu().numpy()
        all_pred.append(out)
        all_actual.append(y.numpy())
all_pred = np.concatenate(all_pred, axis=0)
all_actual = np.concatenate(all_actual, axis=0)

fig, axes = plt.subplots(2, 1, figsize=(12, 6), sharex=True)
axes[0].plot(all_actual[:, 0], label="actual", linewidth=1.5, alpha=0.8)
axes[0].plot(all_pred[:, 0], label="predicted", linewidth=1.5, alpha=0.8, linestyle="--")
axes[0].axhline(0, color="black", linewidth=0.5)
axes[0].set_ylabel("steering"); axes[0].legend(); axes[0].set_ylim(-1.05, 1.05)
mae_s = float(np.mean(np.abs(all_pred[:, 0] - all_actual[:, 0])))
axes[0].set_title(f"steering: 예측 vs 실측  (MAE={mae_s:.4f})")

axes[1].plot(all_actual[:, 1], label="actual", linewidth=1.5, alpha=0.8)
axes[1].plot(all_pred[:, 1], label="predicted", linewidth=1.5, alpha=0.8, linestyle="--")
axes[1].axhline(0, color="black", linewidth=0.5)
axes[1].set_xlabel("seq"); axes[1].set_ylabel("throttle")
axes[1].legend(); axes[1].set_ylim(-0.5, 1.5)
mae_t = float(np.mean(np.abs(all_pred[:, 1] - all_actual[:, 1])))
axes[1].set_title(f"throttle: 예측 vs 실측  (MAE={mae_t:.4f})")
plt.tight_layout()
plt.savefig(OUT / "predictions.png", dpi=120)
plt.close()
print(f"wrote {OUT / 'predictions.png'}  MAE_steering={mae_s:.4f}  MAE_throttle={mae_t:.4f}")

# %% [markdown]
# ## 6. 9개 샘플 frame 에 예측값 라벨링

# %%
sample_idx = np.linspace(0, len(ds) - 1, 9, dtype=int)
fig, axes = plt.subplots(3, 3, figsize=(12, 7))
for ax, idx in zip(axes.flat, sample_idx):
    img = Image.open(OUT / "cam0" / f"{idx:08d}.jpg")
    ax.imshow(img, cmap="gray")
    pred_s = float(all_pred[idx, 0])
    actual_s = float(all_actual[idx, 0])
    color = "green" if abs(pred_s - actual_s) < 0.1 else "red"
    ax.set_title(f"actual={actual_s:+.2f}  pred={pred_s:+.2f}", color=color, fontsize=10)
    ax.axis("off")
plt.suptitle("샘플 frames — 예측이 실측과 일치하면 녹색, 차이 크면 빨강")
plt.tight_layout()
plt.savefig(OUT / "sample_predictions.png", dpi=120)
plt.close()
print(f"wrote {OUT / 'sample_predictions.png'}")

print()
print("=== 완료 ===")
print(f"디렉터리: {OUT}")
for f in sorted(OUT.glob("*.png")):
    print(f"  {f.name:30s}  {f.stat().st_size // 1024:>5} KB")
