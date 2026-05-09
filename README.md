# fsd — Full Self-Driving 1/10 RC Car

1/10 스케일 RC카 (HSP 94118) + NVIDIA Jetson Orin Nano Super + STM32 NUCLEO-H753ZI 기반의
Rust 중심 자율주행 차량 플랫폼.

📖 **자세한 가이드는 [`docs/`](docs/README.md) 참고.** 셋업, 하드웨어, 빌드, 학습, 배포, 트러블슈팅이 분리된 문서로 정리되어 있습니다.

## 디렉터리 구조

```
fsd/
├── protocol/      공유 프로토콜 (no_std 호환). Jetson↔STM32 프레임/명령/텔레메트리.
├── firmware/      STM32 H753ZI no_std Rust 펌웨어 (Embassy). PWM 구동계 + UART.
├── jetson/        Jetson 메인 애플리케이션 (tokio). 시리얼·카메라·로거·추론.
├── ml/            Rust + burn PilotNet (경로 B/C). burn 직접 추론 + ONNX export 시도.
├── ml-py/         Python + PyTorch PilotNet (경로 A). 학습 + ONNX export.
└── cad/           CadQuery 기반 마운트 부품 (카메라/Jetson/NUCLEO 거치대).
```

## 빌드

### 호스트(Jetson) 측
```bash
cargo build --release -p jetson
```

### 펌웨어
```bash
# 사전: rustup target add thumbv7em-none-eabihf, probe-rs 설치
cd firmware
cargo build --release
cargo run --release    # probe-rs 로 플래시 + RTT 로그
```

### ML — 세 가지 경로

본 저장소는 모방학습 모델 학습/배포에 **세 가지 경로**를 제공합니다:

| 경로 | 학습 | 변환 | jetson 추론 백엔드 | 권장도 |
|---|---|---|---|---|
| **A**: PyTorch + ONNX | `ml-py/` (Python) | `torch.onnx.export` | `--features onnx-tensorrt` (ort + TRT EP) | ⭐ 가장 검증됨 |
| **B**: burn 직접 | `ml/` (Rust) | (없음) | `--features burn-inference` | Rust 일관성 |
| **C**: burn → ONNX | `ml/` (Rust) | `fsd-export-onnx`(스텁) | `--features onnx*` | 실험적 — burn ONNX export 불안정 |

#### 경로 A — Python + ONNX (권장)

```bash
cd ml-py
pip install -r requirements.txt
python train.py --manifest ../recordings/run-001/manifest.jsonl --out checkpoints --epochs 30
python export_onnx.py --ckpt checkpoints/best.pt --out ../model.onnx --opset 17
# (Jetson 에서) trtexec --onnx=model.onnx --fp16 --saveEngine=model.engine
```

#### 경로 B — burn 직접

```bash
cargo run --release -p fsd-ml --bin fsd-train -- \
    --manifest ../recordings/run-001/manifest.jsonl --out checkpoints --epochs 30
# 추론은 jetson 측에서 --features burn-inference 로 빌드 + .mpk 모델 사용
```

#### 경로 C — burn → ONNX (실험)

`fsd-export-onnx` 는 현재 스텁입니다. burn 의 ONNX export API 가 안정화되면 채워야 합니다.

#### Jetson 빌드 (백엔드 선택)

```bash
# 가장 무거운 풀세트 (Jetson 권장):
cargo build --release -p fsd-jetson --features "camera,onnx-tensorrt"

# 가벼운 개발용 (PC, CPU only):
cargo build --release -p fsd-jetson --features "onnx"

# burn 백엔드만:
cargo build --release -p fsd-jetson --features "camera,burn-inference"

# 자율주행 실행 — 모델 확장자로 백엔드 자동 선택
./target/release/fsd-jetson drive --model model.onnx       # 경로 A/C
./target/release/fsd-jetson drive --model checkpoints/epoch-030.mpk   # 경로 B
```

### CAD (3D 모델)
```bash
cd cad
pip install cadquery
python camera_mount.py       # build/camera_mount.stl 생성
python jetson_mount.py
python nucleo_mount.py
```

## 통신 프로토콜 요약

- 물리계층: UART, 921600 bps, 8N1 (NUCLEO USART3 ↔ Jetson `/dev/ttyTHS*`)
- 프레이밍: COBS — 0x00 byte를 frame delimiter로 사용
- 페이로드: postcard 직렬화된 `Frame` (`protocol` crate)
- 무결성: CRC-16/IBM 체크섬 (페이로드 끝에 부착)

자세한 메시지 정의는 `protocol/src/lib.rs` 참고.

## 진행 단계

본 저장소는 **스켈레톤 상태**입니다. 컴파일 가능한 기반 코드와 인터페이스가 갖춰져 있으며,
다음 항목은 실제 차량/센서 연결 후 채워야 합니다:

- [ ] firmware: 실제 PWM 핀 매핑, 안전 정지 조건, IMU/ENC 텔레메트리
- [ ] jetson: gstreamer 파이프라인 튜닝, 듀얼 카메라 동기화 검증
- [ ] ml: 데이터셋 통계 기반 정규화, augmentation, hyperparameter sweep, 듀얼 카메라 fusion
- [ ] cad: 실제 차체 치수 기준 마운트 fitment 확인

## 라이선스

MIT OR Apache-2.0
