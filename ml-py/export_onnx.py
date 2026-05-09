"""학습된 PilotNet 체크포인트를 ONNX 로 export.

사용:
  python export_onnx.py --ckpt checkpoints/best.pt --out model.onnx --opset 17

옵션:
  --dynamic-batch : batch 차원을 dynamic 으로. TensorRT 엔진 빌드 시 shape profile 필요.
  --opset N       : ONNX opset 버전. JetPack 6 / TRT 10 = 19 까지. 보수적으로 17 권장.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import torch

import models
from pilotnet import PilotNet  # backward compat


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--ckpt", type=Path, required=True)
    p.add_argument("--out", type=Path, default=Path("model.onnx"))
    p.add_argument("--opset", type=int, default=17)
    p.add_argument("--dynamic-batch", action="store_true",
                   help="batch 차원을 동적으로 export (TRT shape profile 필요)")
    p.add_argument("--stereo", action="store_true",
                   help="6-channel stereo 모델 export. 학습 시 --stereo 였다면 동일하게 켜기")
    p.add_argument("--arch", default="pilotnet", choices=list(models.ARCHS.keys()),
                   help="학습 시 쓴 아키텍처와 동일하게 지정")
    args = p.parse_args()

    model = models.build(args.arch, stereo=args.stereo)
    state = torch.load(args.ckpt, map_location="cpu", weights_only=True)
    model.load_state_dict(state["model"] if "model" in state else state)
    model.eval()

    in_c = models.PilotNet.INPUT_C_STEREO if args.stereo else models.PilotNet.INPUT_C
    dummy = torch.zeros(1, in_c, models.INPUT_H, models.INPUT_W)

    dynamic_axes = {"input": {0: "batch"}, "output": {0: "batch"}} if args.dynamic_batch else None

    torch.onnx.export(
        model,
        dummy,
        args.out,
        input_names=["input"],
        output_names=["output"],
        opset_version=args.opset,
        do_constant_folding=True,
        dynamic_axes=dynamic_axes,
        verbose=False,    # PyTorch 2.x verbose print 가 Windows cp949 와 충돌하는 이모지 사용
    )
    print(f"wrote {args.out}  (opset={args.opset}, dynamic_batch={args.dynamic_batch})")
    print("다음 단계 (Jetson):")
    print(f"  trtexec --onnx={args.out} --fp16 --saveEngine={args.out.with_suffix('.engine')}")


if __name__ == "__main__":
    main()
