# ml-paths — ML 학습/추론 세 가지 경로

본 프로젝트는 학습부터 차량 추론까지 가는 길을 **세 가지** 제공합니다. 같은 모델 아키텍처(PilotNet) 와 같은 데이터 포맷을 공유.

## 결정 트리

```
TensorRT 최고 성능 필요?
  ├ 예 → 경로 A (PyTorch → ONNX → ort+TRT EP)
  └ 아니오 → Rust 일관성 우선?
              ├ 예  → 경로 B (burn 직접)
              └ 실험 → 경로 C (burn → ONNX → ort)
```

## 비교 표

| | A: Python+ONNX | B: burn 직접 | C: burn→ONNX |
|---|---|---|---|
| 학습 언어 | Python | Rust | Rust |
| 변환 | torch.onnx.export | (없음) | burn ONNX export (스텁) |
| jetson 추론 | `ort` (CPU/CUDA/TRT EP) | burn 직접 | `ort` |
| TensorRT 사용 | ✅ | ❌ (burn-tch backend 로 GPU 정도) | ✅ |
| 레퍼런스 자료 | 풍부 (PilotNet, DonkeyCar) | 적음 | 거의 없음 |
| 권장도 | ⭐ 가장 검증됨 | 일관성 좋음 | 실험적 |

## 경로 A — Python + PyTorch + ONNX (권장)

### 학습 (PC 또는 Jetson)
```bash
cd ml-py
. .venv/bin/activate
python train.py --manifest ../recordings/run-001/manifest.jsonl --out checkpoints --epochs 30
```

### ONNX export
```bash
python export_onnx.py --ckpt checkpoints/best.pt --out ../model.onnx --opset 17
```

### (선택) ONNX → TensorRT 엔진 (Jetson 에서)
```bash
trtexec --onnx=model.onnx --fp16 --saveEngine=model.engine \
        --minShapes=input:1x3x66x200 --optShapes=input:1x3x66x200 --maxShapes=input:1x3x66x200
```

`.engine` 은 Jetson 하드웨어/TRT 버전별 — 다른 Jetson 에서는 재빌드 필요.

### 추론 (Jetson)
```bash
cargo build --release -p fsd-jetson --features "camera,onnx-tensorrt"
./target/release/fsd-jetson drive --model model.onnx
```

ort 가 첫 실행 시 자동으로 TRT 엔진 빌드/캐시. `.engine` 직접 로드 원하면 `ort` 의 TensorRT EP 옵션 설정.

## 경로 B — Rust + burn 직접

### 학습
```bash
# CPU (ndarray)
cargo run --release -p fsd-ml --bin fsd-train -- \
    --manifest ../recordings/run-001/manifest.jsonl --out checkpoints --epochs 30

# CUDA (libtorch)
cargo run --release -p fsd-ml --no-default-features --features tch-cuda --bin fsd-train -- ...
```

### 추론 검증 (단일 이미지)
```bash
cargo run --release -p fsd-ml --bin fsd-infer -- \
    --ckpt checkpoints/epoch-030.mpk --image sample.jpg
```

### 차량 배포
```bash
cargo build --release -p fsd-jetson --features "camera,burn-inference"
./target/release/fsd-jetson drive --model checkpoints/epoch-030.mpk
```

## 경로 C — burn → ONNX (실험)

현재 `fsd-export-onnx` 는 **스텁**입니다. burn 의 ONNX export 가 안정화되면 채울 자리. 그 전엔 경로 A 사용 권장.

```bash
cargo run --release -p fsd-ml --bin fsd-export-onnx -- --ckpt ... --out model.onnx
# (스텁: 사용 안내만 출력)
```

## E2E smoke test

실 데이터 없이 학습→export→추론 사이클을 검증:

```bash
cd ml-py
python smoke.py                # 약 1–3 분
python smoke.py --keep         # 디버깅용 (임시 디렉터리 유지)
```

내부 동작:
1. `synthetic.py` — 그라디언트 이미지 + 사인 곡선 steering 의 200 프레임 생성
2. `train.py` — 25 epoch 학습 (synthetic 데이터 기준 MAE ~0.1 수준)
3. `export_onnx.py` — ONNX 변환 (`verbose=False` 로 Windows cp949 충돌 회피)
4. `replay.py` — Python onnxruntime 으로 모델 추론, CSV 출력 (PC/CI 어디서든 동작)
5. MAE_steering < 0.4 + avg latency < 200 ms 검증

**검증된 환경**: Windows 11 + Python 3.12 + PyTorch 2.11 + onnxruntime 1.25 (CPU). 1분 이내 완료.

Jetson 배포 시점에는 `cargo run -p fsd-jetson --features onnx-tensorrt -- replay` 로 실 백엔드 (TensorRT EP) 검증 가능. Python smoke 가 모델 자체를 보장하고, Rust replay 가 배포 환경 추가 검증.

용도: 모델 아키텍처 / 데이터 로더 / ONNX export 변경 후 회귀 검증, CI 에 통합 가능.

## Multi-camera fusion (stereo)

`--stereo` 플래그로 cam0+cam1 을 6채널 입력으로 학습:

```bash
# synthetic 에서도 cam1 이 parallax-shifted 되도록.
python synthetic.py --out ../recordings/test --n 200 --stereo

python train.py --manifest ../recordings/test/manifest.jsonl --out ckpts --epochs 30 --stereo
python export_onnx.py --ckpt ckpts/best.pt --out model.onnx --stereo
# replay 는 자동으로 model.onnx 의 입력 채널을 보고 mono/stereo 결정
python replay.py --recording ../recordings/test --model model.onnx
```

**구현 디테일**:
- `PilotNet(stereo=True)` 면 첫 conv 입력 채널 6, mean/std buffer 도 6채널로 복제
- `DrivingDataset(stereo=True)` 가 cam0+cam1 을 (6, H, W) 로 concat 반환
- AugmentingDataset hflip: 6채널이면 cam0 ↔ cam1 도 같이 swap (flipped left ≈ right)
- ONNX 의 입력 shape 가 (1, 6, 66, 200) 으로 export → jetson 측은 자동 인식

**한계**:
- 현재 `ml/`(Rust burn) 측은 mono 만. stereo 학습은 Python 만 지원
- jetson 인퍼런스에서 cam1 jpeg 를 6채널 텐서로 합치는 헬퍼는 미구현 — TODO

## 모델 아키텍처 선택 (`--arch`)

| arch | 파라미터 | 특징 |
|---|---|---|
| `pilotnet` (기본) | ~252k | NVIDIA PilotNet 변형. 전형적, 풍부한 데이터에서 안정적 |
| `tiny` | ~50k | 경량화. 작은 데이터셋·빠른 추론 필요 시. CPU 에서도 1ms 이하 |

```bash
python train.py --arch tiny --manifest ... --out ckpts
python export_onnx.py --arch tiny --ckpt ckpts/best.pt --out tiny.onnx
python compare_archs.py    # 같은 데이터로 모든 arch 비교 표
```

`compare_archs.py` 출력 예 (200 frames, 20 epoch):
```
arch          params   onnx KB   train s   MAE_s   MAE_t  mean us  p50 us  p99 us
pilotnet     252,230       5.5       7.1  0.2089  0.1037      241     233     406
tiny         230,474       4.4       6.0  0.0519  0.0226      160     157     315
```
→ 작은 데이터(200)에선 tiny 가 PilotNet 보다 빠르고 정확.

## 운전 시뮬레이터 (실차 없이)

`drive_sim.py` 가 키보드로 운전 가능한 단순 도로 뷰를 띄움. cv2 윈도우 + bicycle model.

```bash
python drive_sim.py --out ../recordings/sim-001 --duration 120
```

조작: `←→` 조향, `↑↓` 스로틀, `space` 브레이크, `s` 녹화 토글, `q` 종료.
출력 manifest 는 실차 record 와 같은 포맷 → 그대로 train.py 입력으로 사용.

용도:
- ML 파이프라인 sanity 검증 (실차 없이 더 풍부한 데이터)
- 운전 패턴 연습 (입력 손맛)
- 새 augmentation 효과 측정

## 추론 벤치마크

ONNX 모델의 latency 분포(p50/p95/p99) + throughput 측정:

```bash
python bench.py --model model.onnx                     # CPU EP
python bench.py --model model.onnx --provider cuda     # CUDA EP (NVIDIA GPU 필요)
python bench.py --model model.onnx --provider tensorrt # TensorRT EP (Jetson)
```

출력 예시 (Windows CPU, PilotNet):
```
mean      :       251 us
p99       :       445 us
throughput:      3974 fps
50 Hz budget headroom: 79.7x
```

p99 가 20 ms 초과 시 자동 경고 — 50 Hz 루프(20 ms 예산) 못 따라간다는 뜻. Jetson 에서 백엔드 비교에 활용.

## 모델 일관성

`ml/src/model.rs` (burn) 과 `ml-py/pilotnet.py` (PyTorch) 는 **같은 layer 배치**여야 합니다.
변경 시 양쪽 모두 갱신. 입력 (3, 66, 200) 출력 (2,) 고정.

## 향후 추가 (TODO)

- 데이터 augmentation (양 경로)
- 데이터셋 mean/std 기반 정규화 (현재는 단순 0..1)
- early stopping, LR scheduler
- 듀얼 카메라 fusion (현재 cam0 만 사용)
- INT8 양자화 (calibration dataset 필요)
- Visual SLAM (별도 모듈, 명세서 3.2.2)
