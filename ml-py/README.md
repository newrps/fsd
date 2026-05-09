# ml-py — PyTorch 학습 + ONNX export (경로 A)

Jetson 에서 가장 검증된 자율주행 학습/배포 파이프라인의 **학습 절반**을 담당.

```
[ml-py 영역 — Python]                 [jetson 영역 — Rust]
recordings/manifest.jsonl              model.onnx
       │                                   │
       │  train.py                         │  ort + CUDA/TensorRT EP
       ▼                                   ▼
   pilotnet.pt  ─── export_onnx.py ──▶  실시간 추론
```

## 셋업

### PC (학습용)

```bash
cd ml-py
python -m venv .venv
. .venv/bin/activate    # Windows: .venv\Scripts\activate
pip install -r requirements.txt
```

### Jetson (학습 또는 ONNX 빌드용)

Jetson 에서는 NVIDIA 가 제공하는 JetPack 호환 PyTorch wheel 을 써야 합니다.
`pip install torch` 가 아니라 https://forums.developer.nvidia.com 의 안내에 따라
`torch-2.x.x-cp310-cp310-linux_aarch64.whl` 같은 사전 빌드 wheel 설치.

## 사용

```bash
# 학습
python train.py --manifest ../recordings/run-001/manifest.jsonl \
                --out checkpoints --epochs 30 --batch-size 64

# ONNX export (가장 좋은 체크포인트로)
python export_onnx.py --ckpt checkpoints/best.pt --out model.onnx --opset 17

# (Jetson 에서) ONNX → TensorRT 엔진
trtexec --onnx=model.onnx --fp16 --saveEngine=model.engine \
        --minShapes=input:1x3x66x200 --optShapes=input:1x3x66x200 --maxShapes=input:1x3x66x200
```

이후 `model.onnx` 또는 `model.engine` 을 jetson 의 `fsd-jetson drive --model ...` 으로 사용.

## 모델

`pilotnet.py` 의 `PilotNet` 은 NVIDIA 의 End-to-End 논문 구조를 따릅니다 (입력 3×66×200, 출력 [steering, throttle]).
**`ml/`(burn) 의 `PilotNet` 과 정확히 동일한 layer 배치** — 같은 입력/출력 형식이라 데이터셋·평가 스크립트를 공유 가능.
