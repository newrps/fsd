"""ONNX 모델 추론 latency 벤치마크.

목적:
  - 모델/백엔드/하드웨어 별 추론 성능 비교
  - 50 Hz 실시간 루프(20 ms 예산) 안에 들어오는지 확인
  - p50/p95/p99 백분위 측정으로 worst-case 검증

사용:
  python bench.py --model model.onnx                       # CPU EP, 1000회
  python bench.py --model model.onnx --provider cuda       # CUDA EP
  python bench.py --model model.onnx --provider tensorrt   # TensorRT EP (Jetson)
  python bench.py --model model.onnx --warmup 50 --iters 5000

`fsd-jetson` Rust 측 추론과 같은 입력 형식(0..1 RGB CHW (1,3,66,200)) 사용.
"""

from __future__ import annotations

import argparse
import statistics
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort


PROVIDERS = {
    "cpu":      ["CPUExecutionProvider"],
    "cuda":     ["CUDAExecutionProvider", "CPUExecutionProvider"],
    "tensorrt": ["TensorrtExecutionProvider", "CUDAExecutionProvider", "CPUExecutionProvider"],
}


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--model", type=Path, required=True)
    p.add_argument("--provider", choices=list(PROVIDERS), default="cpu")
    p.add_argument("--warmup", type=int, default=20, help="첫 N회는 캐시 워밍업으로 제외")
    p.add_argument("--iters", type=int, default=1000)
    p.add_argument("--seed", type=int, default=42)
    args = p.parse_args()

    providers = PROVIDERS[args.provider]
    print(f"loading {args.model}  providers={providers}")
    sess = ort.InferenceSession(str(args.model), providers=providers)
    actual_provider = sess.get_providers()[0]
    print(f"actual EP: {actual_provider}")

    input_meta = sess.get_inputs()[0]
    input_name = input_meta.name
    output_name = sess.get_outputs()[0].name

    # 입력 형태 추론. dynamic 차원이 있으면 1×3×66×200 으로 가정.
    shape = tuple(s if isinstance(s, int) and s > 0 else d for s, d in zip(
        input_meta.shape, [1, 3, 66, 200],
    ))
    print(f"input  : {input_name} {shape}")
    print(f"output : {output_name}")

    rng = np.random.default_rng(args.seed)
    dummy = rng.random(shape, dtype=np.float32)

    # warmup
    print(f"warmup {args.warmup} iters...")
    for _ in range(args.warmup):
        sess.run([output_name], {input_name: dummy})

    # 측정
    print(f"measuring {args.iters} iters...")
    latencies_us = []
    t_start = time.perf_counter()
    for _ in range(args.iters):
        t0 = time.perf_counter_ns()
        sess.run([output_name], {input_name: dummy})
        latencies_us.append((time.perf_counter_ns() - t0) / 1000.0)
    elapsed_s = time.perf_counter() - t_start

    latencies_us.sort()
    p50 = latencies_us[len(latencies_us) // 2]
    p95 = latencies_us[int(len(latencies_us) * 0.95)]
    p99 = latencies_us[int(len(latencies_us) * 0.99)]
    mean = statistics.mean(latencies_us)
    stdev = statistics.stdev(latencies_us) if len(latencies_us) > 1 else 0.0

    fps = args.iters / elapsed_s if elapsed_s > 0 else float("inf")

    print()
    print(f"=== {args.provider} ({actual_provider}) ===")
    print(f"iters     : {args.iters}")
    print(f"mean      : {mean:>9.0f} us")
    print(f"stdev     : {stdev:>9.0f} us")
    print(f"p50       : {p50:>9.0f} us")
    print(f"p95       : {p95:>9.0f} us")
    print(f"p99       : {p99:>9.0f} us")
    print(f"max       : {latencies_us[-1]:>9.0f} us")
    print(f"throughput: {fps:>9.0f} fps")
    margin_50hz = 20_000 / mean if mean > 0 else float("inf")
    print(f"50 Hz budget headroom: {margin_50hz:.1f}x")
    if p99 > 20_000:
        print("WARN: p99 latency > 20 ms — 50 Hz 루프 안 들어올 수 있음")


if __name__ == "__main__":
    main()
