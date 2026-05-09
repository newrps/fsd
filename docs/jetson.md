# jetson — Jetson 메인 애플리케이션

`jetson/` 디렉터리. tokio 기반 비동기 Rust 앱.

## 세 가지 실행 모드

```bash
fsd-jetson serve                                                       # 시리얼 브리지만 (디버깅)
fsd-jetson record --out recordings/run-001 --fps 30                    # 데이터 수집
fsd-jetson drive  --model model.onnx                                   # 자율 주행 (실차)
fsd-jetson replay --recording recordings/run-001 --model model.onnx    # 오프라인 모델 검증
```

공통 옵션:
- `--serial /dev/ttyACM0` (또는 `/dev/ttyTHS1`)
- `--baud 921600` (기본값)

## Feature Flag 매트릭스

| Feature | 켜면 | 끄면 (default) |
|---|---|---|
| `camera` | 듀얼 IMX219 GStreamer 캡처 활성 | 카메라 미사용 (PC 개발용) |
| `gamepad` | gilrs 로 USB 게임패드 입력 | record 모드에서 --input gamepad 거부 |
| `onnx` | `ort` (CPU EP) 로 .onnx 추론 | drive 모드에서 .onnx 거부 |
| `onnx-cuda` | onnx + CUDA EP | (Jetson 권장) |
| `onnx-tensorrt` | onnx + TensorRT EP | (Jetson 최고 성능) |
| `burn-inference` | fsd-ml burn 으로 .mpk 추론 | drive 모드에서 .mpk 거부 |

## 빌드 예

```bash
# PC 개발 (시리얼만 테스트)
cargo build --release -p fsd-jetson

# PC 에서 ONNX 추론 테스트
cargo build --release -p fsd-jetson --features onnx

# Jetson — 데이터 수집용 (게임패드 + 카메라)
cargo build --release -p fsd-jetson --features "camera,gamepad"

# Jetson — 자율주행용 (TensorRT + 카메라)
cargo build --release -p fsd-jetson --features "camera,onnx-tensorrt"

# burn 직접 추론
cargo build --release -p fsd-jetson --features "camera,burn-inference"

# 풀스택 — 수집·학습·추론 한 번에 (모든 기능, 빌드 무거움)
cargo build --release -p fsd-jetson --features "camera,gamepad,onnx-tensorrt,burn-inference"
```

## 모드별 동작

### `serve`
- 100 ms 마다 NEUTRAL 명령 송신 (heartbeat 역할)
- 수신한 텔레메트리 stdout 로그
- 시리얼 통신 자체 검증용

### `record`
- camera feature 필요 (없으면 경고만)
- `--input rc|gamepad|auto` 로 입력 소스 선택
  - `rc`: STM32 펌웨어가 캡처한 RC 수신기 PWM 을 텔레메트리로 받아 사용
  - `gamepad`: USB 게임패드 (gilrs, `--features gamepad` 필수)
  - `auto`(기본): gamepad 가 init 되면 gamepad, 아니면 RC 로 fallback
- 50 Hz 로 입력 → DriveCommand → STM32 송신 + logger 에 동시 기록
- 듀얼 카메라 프레임 + 그 시점 명령 → `<out>/manifest.jsonl` + `cam0/*.jpg` + `cam1/*.jpg`

### `drive`
- 모델 확장자로 백엔드 자동 선택 (`.onnx` → ort, `.mpk` → burn)
- 카메라 프레임 → CHW float (200×66) 정규화 → `Predictor::predict` → DriveCommand 송신
- `Ctrl-C` 로 정상 종료 (시리얼 NEUTRAL 송출 후 close)
- **`--calib stereo_calib.json` + `--features slam-opencv`** 시 듀얼 카메라 obstacle 감지:
  - 별도 thread 에서 stereo SGBM 처리 (50 Hz 추론 루프 비차단)
  - obstacle_ratio 가 slow 임계 (15%) 초과 시 throttle 자동 감속, stop 임계 (30%) 시 정지
  - 자세한 정책은 [slam.md](slam.md) 참고

### `replay`
- 카메라/STM32 **불필요** — recordings 디렉터리만 있으면 동작
- manifest.jsonl 의 각 frame 을 model 에 통과시켜 예측 vs 실측을 CSV 출력
- 출력: `<recording>/replay.csv` (또는 `--out` 지정)
- 콘솔 출력 요약:
  - 처리 샘플 수, skip 수
  - 평균 추론 latency (µs)
  - MAE_steering, MAE_throttle
- 용도:
  - 새 모델 학습 후 즉시 sanity 검사 (예측이 너무 NaN/한쪽 쏠림 등)
  - 추론 백엔드(ort vs burn vs TRT EP) 성능 비교
  - 모델 architecture 변경 시 회귀 검증

## 로그

`tracing-subscriber` 사용. 환경 변수:

```bash
RUST_LOG=info  fsd-jetson serve         # 기본
RUST_LOG=debug fsd-jetson record --out ...
RUST_LOG=fsd_jetson=trace,fsd_protocol=debug fsd-jetson drive --model ...
```

## 시리얼 권한 (Linux)

```bash
sudo usermod -aG dialout $USER     # 로그아웃 후 재로그인
```

## 디렉터리 레이아웃 (record 출력)

```
recordings/run-001/
├── manifest.jsonl     # 한 줄 = Sample (seq, t, steering, throttle, cam0, cam1)
├── cam0/
│   ├── 00000000.jpg
│   ├── 00000001.jpg
│   └── ...
└── cam1/
    └── ...
```

`ml-py/dataset.py` 와 `ml/src/data.rs` 가 같은 포맷을 읽음.
