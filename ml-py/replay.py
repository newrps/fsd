"""ONNX 모델 + manifest 로 오프라인 replay (jetson 의 fsd-jetson replay 와 동일 검증).

용도:
  - PC 에서 ONNX 모델 sanity 체크 (cargo 빌드 안 거치고)
  - smoke.py 의 4단계가 ort Windows 빌드 이슈 회피
  - CI 환경에서 빠른 검증

jetson 의 Rust replay 와 동일한 입력 전처리(0..1 RGB CHW)·동일한 출력 형식 보장.
"""

from __future__ import annotations

import argparse
import csv
import json
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
from PIL import Image


INPUT_W, INPUT_H = 200, 66


def model_input_channels(sess: ort.InferenceSession) -> int:
    """모델 입력 첫 차원의 채널 수를 가져온다 (3=mono, 6=stereo)."""
    shape = sess.get_inputs()[0].shape
    # shape = [batch, C, H, W]; batch 가 dynamic 일 수 있음.
    return int(shape[1])


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--recording", type=Path, required=True)
    p.add_argument("--model", type=Path, required=True)
    p.add_argument("--out", type=Path)
    args = p.parse_args()

    out_path = args.out if args.out else (args.recording / "replay.csv")
    manifest = args.recording / "manifest.jsonl"

    sess = ort.InferenceSession(str(args.model), providers=["CPUExecutionProvider"])
    input_name = sess.get_inputs()[0].name
    output_name = sess.get_outputs()[0].name
    in_c = model_input_channels(sess)
    is_stereo = (in_c == 6)
    print(f"loaded {args.model}  inputs=[{input_name}, C={in_c}]  outputs=[{output_name}]")
    if is_stereo:
        print("  → stereo mode: cam0+cam1 6-channel")

    rows = []
    total = 0
    bad = 0
    sum_lat_us = 0.0
    sum_abs_err_s = 0.0
    sum_abs_err_t = 0.0

    with manifest.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            cam0 = Path(obj["cam0"])
            path0 = cam0 if cam0.is_absolute() else args.recording / cam0
            try:
                img0 = Image.open(path0).convert("RGB").resize((INPUT_W, INPUT_H), Image.BILINEAR)
            except OSError:
                bad += 1
                continue
            arr0 = np.asarray(img0, dtype=np.float32) / 255.0  # (H, W, 3)
            if is_stereo:
                cam1_str = obj.get("cam1")
                if cam1_str:
                    cam1 = Path(cam1_str)
                    path1 = cam1 if cam1.is_absolute() else args.recording / cam1
                    try:
                        img1 = Image.open(path1).convert("RGB").resize((INPUT_W, INPUT_H), Image.BILINEAR)
                        arr1 = np.asarray(img1, dtype=np.float32) / 255.0
                    except OSError:
                        arr1 = arr0
                else:
                    arr1 = arr0
                arr = np.concatenate([arr0, arr1], axis=2)  # (H, W, 6)
            else:
                arr = arr0
            chw = arr.transpose(2, 0, 1)[None, :, :, :]  # (1, C, H, W)

            t0 = time.perf_counter_ns()
            out = sess.run([output_name], {input_name: chw})[0]
            lat_us = (time.perf_counter_ns() - t0) / 1000.0

            pred_s = float(np.clip(out[0, 0], -1.0, 1.0))
            pred_t = float(np.clip(out[0, 1], -1.0, 1.0))
            actual_s = float(obj.get("steering", 0.0))
            actual_t = float(obj.get("throttle", 0.0))

            sum_lat_us += lat_us
            sum_abs_err_s += abs(pred_s - actual_s)
            sum_abs_err_t += abs(pred_t - actual_t)
            total += 1

            rows.append({
                "seq": obj["seq"],
                "actual_steering": actual_s,
                "actual_throttle": actual_t,
                "pred_steering": pred_s,
                "pred_throttle": pred_t,
                "latency_us": int(lat_us),
            })

    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=[
            "seq", "actual_steering", "actual_throttle",
            "pred_steering", "pred_throttle", "latency_us",
        ])
        writer.writeheader()
        writer.writerows(rows)

    avg_lat = sum_lat_us / total if total else 0
    mae_s = sum_abs_err_s / total if total else 0
    mae_t = sum_abs_err_t / total if total else 0
    print(f"samples processed = {total} (bad skipped = {bad})")
    print(f"avg latency       = {avg_lat:.0f} us")
    print(f"MAE steering      = {mae_s:.4f}")
    print(f"MAE throttle      = {mae_t:.4f}")
    print(f"CSV               = {out_path}")


if __name__ == "__main__":
    main()
