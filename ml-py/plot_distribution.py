"""manifest.jsonl 의 데이터 분포를 시각화 — 데이터 품질 디버깅용.

체크 항목:
  - steering / throttle 히스토그램 (좌우 균형 + 변화 범위)
  - 시간 순 시계열 (운전 패턴 + 갑작스런 점프 감지)
  - frame interval (캡처 jitter — 너무 들쭉날쭉하면 동기화 문제)
  - steering vs throttle 산점도 (운전 스타일)

사용:
  python plot_distribution.py --manifest ../recordings/run-001/manifest.jsonl
  → ../recordings/run-001/distribution.png 생성

학습 시작 전 반드시 한 번 확인 권장. steering 분포가 한쪽으로 크게 쏠려있으면 augmentation 으로
좌우 flip 만 으론 부족할 수 있고, 추가 데이터 수집 필요.
"""

from __future__ import annotations

import argparse
import json
from datetime import datetime
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np


def load(manifest: Path) -> dict[str, np.ndarray]:
    seqs, ts, steerings, throttles = [], [], [], []
    with manifest.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            seqs.append(int(obj["seq"]))
            try:
                t_str = obj["t"].replace("Z", "+00:00")
                ts.append(datetime.fromisoformat(t_str).timestamp())
            except (KeyError, ValueError):
                ts.append(float(len(ts)) / 30.0)
            steerings.append(float(obj.get("steering", 0.0)))
            throttles.append(float(obj.get("throttle", 0.0)))
    return {
        "seq": np.array(seqs),
        "t": np.array(ts),
        "steering": np.array(steerings),
        "throttle": np.array(throttles),
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--out", type=Path, help="기본: <manifest 디렉터리>/distribution.png")
    args = p.parse_args()

    out = args.out if args.out else (args.manifest.parent / "distribution.png")
    data = load(args.manifest)
    n = len(data["seq"])
    if n == 0:
        raise SystemExit(f"manifest empty: {args.manifest}")

    fig, axes = plt.subplots(2, 3, figsize=(16, 9))
    fig.suptitle(f"{args.manifest.parent.name} — {n} samples", fontsize=14)

    # 1. steering histogram
    ax = axes[0, 0]
    ax.hist(data["steering"], bins=40, color="tab:blue", edgecolor="black")
    ax.axvline(0, color="red", linestyle="--", linewidth=0.8)
    ax.set_title("steering distribution")
    ax.set_xlabel("steering [-1, +1]")
    ax.set_ylabel("count")
    ax.set_xlim(-1.05, 1.05)
    s_mean = float(np.mean(data["steering"]))
    s_std = float(np.std(data["steering"]))
    ax.text(0.02, 0.95, f"mean={s_mean:+.3f}\nstd={s_std:.3f}",
            transform=ax.transAxes, va="top",
            bbox=dict(boxstyle="round", facecolor="white", alpha=0.7))

    # 2. throttle histogram
    ax = axes[0, 1]
    ax.hist(data["throttle"], bins=40, color="tab:orange", edgecolor="black")
    ax.axvline(0, color="red", linestyle="--", linewidth=0.8)
    ax.set_title("throttle distribution")
    ax.set_xlabel("throttle [-1, +1]")
    ax.set_xlim(-1.05, 1.05)
    t_mean = float(np.mean(data["throttle"]))
    t_std = float(np.std(data["throttle"]))
    ax.text(0.02, 0.95, f"mean={t_mean:+.3f}\nstd={t_std:.3f}",
            transform=ax.transAxes, va="top",
            bbox=dict(boxstyle="round", facecolor="white", alpha=0.7))

    # 3. frame interval (capture jitter)
    ax = axes[0, 2]
    if len(data["t"]) > 1:
        intervals_ms = np.diff(data["t"]) * 1000.0
        ax.hist(intervals_ms, bins=40, color="tab:green", edgecolor="black")
        median = float(np.median(intervals_ms))
        ax.axvline(median, color="red", linestyle="--", linewidth=0.8, label=f"median={median:.1f}ms")
        ax.set_title("frame interval (capture jitter)")
        ax.set_xlabel("ms")
        ax.legend()

    # 4. steering over time (seq)
    ax = axes[1, 0]
    ax.plot(data["seq"], data["steering"], color="tab:blue", linewidth=0.5)
    ax.axhline(0, color="black", linewidth=0.3)
    ax.set_title("steering over seq")
    ax.set_xlabel("seq")
    ax.set_ylabel("steering")
    ax.set_ylim(-1.05, 1.05)

    # 5. throttle over time
    ax = axes[1, 1]
    ax.plot(data["seq"], data["throttle"], color="tab:orange", linewidth=0.5)
    ax.axhline(0, color="black", linewidth=0.3)
    ax.set_title("throttle over seq")
    ax.set_xlabel("seq")
    ax.set_ylabel("throttle")
    ax.set_ylim(-1.05, 1.05)

    # 6. steering vs throttle scatter (운전 스타일)
    ax = axes[1, 2]
    ax.scatter(data["steering"], data["throttle"], s=2, alpha=0.4, color="tab:purple")
    ax.axhline(0, color="black", linewidth=0.3)
    ax.axvline(0, color="black", linewidth=0.3)
    ax.set_title("steering vs throttle")
    ax.set_xlabel("steering")
    ax.set_ylabel("throttle")
    ax.set_xlim(-1.05, 1.05)
    ax.set_ylim(-1.05, 1.05)

    plt.tight_layout()
    plt.savefig(out, dpi=120)
    print(f"wrote {out}")
    print(f"  steering mean={s_mean:+.3f} std={s_std:.3f}")
    print(f"  throttle mean={t_mean:+.3f} std={t_std:.3f}")
    if abs(s_mean) > 0.1:
        print(f"  ⚠ steering mean 이 0 에서 멀음 ({s_mean:+.3f}) — 좌/우 균형 데이터 부족 가능성")
    if s_std < 0.1:
        print(f"  ⚠ steering std 가 작음 ({s_std:.3f}) — 코너 데이터 거의 없음")


if __name__ == "__main__":
    main()
