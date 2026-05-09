"""End-to-end smoke test.

순서:
  1. synthetic 데이터 생성 (~200 frames, 빠르게)
  2. PyTorch PilotNet 학습 (3 epochs)
  3. ONNX export
  4. fsd-jetson replay 로 검증
  5. MAE/latency 임계 통과 확인

회귀 테스트로 활용. CI 에서 호출 가능.

사용:
  python smoke.py
  python smoke.py --keep   # 임시 디렉터리 유지 (디버깅)
"""

from __future__ import annotations

import argparse
import csv
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# Windows 의 cp949 코드페이지가 PyTorch/cargo 의 utf-8 출력과 충돌해 깨지는 것을 방지.
os.environ.setdefault("PYTHONIOENCODING", "utf-8")


HERE = Path(__file__).resolve().parent


def run(cmd: list, **kwargs) -> subprocess.CompletedProcess:
    print(f"  $ {' '.join(str(c) for c in cmd)}")
    return subprocess.run(cmd, check=True, **kwargs)


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--keep", action="store_true", help="임시 디렉터리 유지")
    p.add_argument("--n", type=int, default=200)
    p.add_argument("--epochs", type=int, default=25,
                   help="모델 학습 epoch. synthetic 200 frames + augmentation 시 25 epoch 기준 MAE ~0.3")
    p.add_argument("--mae-threshold", type=float, default=0.4,
                   help="replay MAE_steering 임계")
    p.add_argument("--latency-threshold-us", type=int, default=200_000,
                   help="평균 추론 latency 임계. 200ms.")
    args = p.parse_args()

    work = Path(tempfile.mkdtemp(prefix="fsd-smoke-"))
    print(f"work dir: {work}")
    try:
        recording = work / "recording"
        ckpts = work / "ckpts"
        model_onnx = work / "model.onnx"

        print("\n[1/4] synthetic 데이터 생성")
        run([
            sys.executable, str(HERE / "synthetic.py"),
            "--out", str(recording),
            "--n", str(args.n),
        ])

        print(f"\n[2/4] PyTorch 학습 ({args.epochs} epochs)")
        run([
            sys.executable, str(HERE / "train.py"),
            "--manifest", str(recording / "manifest.jsonl"),
            "--out", str(ckpts),
            "--epochs", str(args.epochs),
            "--batch-size", "32",
            "--workers", "0",  # CI 환경에서 multiprocessing 회피
        ])

        print("\n[3/4] ONNX export")
        run([
            sys.executable, str(HERE / "export_onnx.py"),
            "--ckpt", str(ckpts / "best.pt"),
            "--out", str(model_onnx),
            "--opset", "17",
        ])

        print("\n[4/4] ONNX replay (Python onnxruntime)")
        # Python 으로 검증 — cargo build 의 ort Windows 이슈 회피.
        # Jetson 실배포 시엔 fsd-jetson replay (Rust) 로 동일 검증 가능.
        run([
            sys.executable, str(HERE / "replay.py"),
            "--recording", str(recording),
            "--model", str(model_onnx),
            "--out", str(work / "replay.csv"),
        ])

        # CSV 분석.
        rows = []
        with (work / "replay.csv").open() as f:
            reader = csv.DictReader(f)
            for row in reader:
                rows.append({
                    "actual_s": float(row["actual_steering"]),
                    "pred_s": float(row["pred_steering"]),
                    "actual_t": float(row["actual_throttle"]),
                    "pred_t": float(row["pred_throttle"]),
                    "lat": int(row["latency_us"]),
                })
        if not rows:
            print("FAIL: replay CSV 가 비어있음")
            return 1

        mae_s = sum(abs(r["actual_s"] - r["pred_s"]) for r in rows) / len(rows)
        mae_t = sum(abs(r["actual_t"] - r["pred_t"]) for r in rows) / len(rows)
        avg_lat = sum(r["lat"] for r in rows) / len(rows)

        print("\n=== smoke test 결과 ===")
        print(f"samples       : {len(rows)}")
        print(f"MAE steering  : {mae_s:.4f}  (threshold {args.mae_threshold})")
        print(f"MAE throttle  : {mae_t:.4f}")
        print(f"avg latency   : {avg_lat:.0f} us  (threshold {args.latency_threshold_us})")

        ok = True
        if mae_s > args.mae_threshold:
            print(f"FAIL: MAE_steering {mae_s:.4f} > {args.mae_threshold}")
            ok = False
        if avg_lat > args.latency_threshold_us:
            print(f"FAIL: avg latency {avg_lat:.0f} us > {args.latency_threshold_us}")
            ok = False

        if ok:
            print("PASS")
            return 0
        return 1

    finally:
        if not args.keep:
            shutil.rmtree(work, ignore_errors=True)
        else:
            print(f"\n임시 디렉터리 유지: {work}")


if __name__ == "__main__":
    sys.exit(main())
