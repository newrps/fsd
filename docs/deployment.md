# deployment — 자율주행 배포

학습된 모델을 실제 차량에 올려서 자율주행 시키는 절차.

## 사전 체크리스트

- [ ] 펌웨어 플래시됨 (firmware.md 참고)
- [ ] Jetson 부팅 + 시리얼 통신 OK (`fsd-jetson serve` 로 텔레메트리 보임)
- [ ] 카메라 두 대 모두 캡처 OK (`gst-launch-1.0` 로 단독 검증)
- [ ] 학습 데이터 충분 (manifest 10k+ 줄, 분포 균형)
- [ ] 학습 완료 + best 체크포인트 손에 있음
- [ ] **비상 정지 수단 준비** (사람이 차에 닿을 수 있는 거리 + 물리 스위치)

## 배포 단계

### 1단계: ONNX export (경로 A 기준)

PC 또는 Jetson 에서:
```bash
cd ml-py
python export_onnx.py --ckpt checkpoints/best.pt --out ../model.onnx --opset 17
```

### 2단계: Jetson 으로 소스 + 모델 전송

자동 스크립트 사용:

```bash
export FSD_HOST=jetson@<ip>
export FSD_MODEL=./model.onnx
export FSD_CALIB=./stereo_calib.json    # 선택
./scripts/deploy.sh                      # rsync + 원격 빌드
```

또는 수동:

```bash
scp model.onnx jetson@<ip>:/home/jetson/fsd/
rsync -a --exclude target/ ./ jetson@<ip>:/home/jetson/fsd/
```

### 3단계: TRT 엔진 사전 빌드 (선택, 시작 시간 단축)

Jetson 에서:
```bash
trtexec --onnx=model.onnx --fp16 --saveEngine=model.engine
```

이 단계 생략해도 ort 가 첫 실행 시 자동 빌드. 단 첫 실행이 1–2분 길어짐.

### 4단계: jetson 앱 빌드 (Jetson 위에서)

```bash
cd /home/jetson/fsd
export ORT_DYLIB_PATH=/usr/lib/aarch64-linux-gnu/libonnxruntime.so   # JetPack ORT 사용 시
cargo build --release -p fsd-jetson --features "camera,onnx-tensorrt"
```

### 5단계: 자율주행 실행

```bash
sudo ./target/release/fsd-jetson drive \
    --serial /dev/ttyTHS1 \
    --baud 921600 \
    --model model.onnx \
    --calib stereo_calib.json   # 선택, slam-opencv feature 빌드 시 obstacle 감지 활성
```

`--calib` + `--features slam-opencv` 빌드: 듀얼 카메라로 전방 obstacle 비율 감지 → throttle 자동 감속/정지 (자세한 정책: [slam.md](slam.md)).

`sudo` 는 시리얼 권한 안 풀었을 때만 필요. dialout 그룹 추가하면 불필요.

### 5.4단계: 추론 latency 검증 (Jetson)

```bash
cd ml-py
python bench.py --model ../model.onnx --provider tensorrt --iters 2000
```

`p99 < 20 ms` 확인 — 50 Hz 루프 안에 들어와야 함. p99 가 18 ms 이상이면 모델이 너무 크거나 TRT 변환 미흡.

### 5.5단계: 사전 sanity 검사 (replay)

차량 위에 올라가기 전 **반드시** replay 로 모델이 의미 있는 출력을 내는지 확인:

```bash
./target/release/fsd-jetson replay \
    --recording recordings/run-001 \
    --model model.onnx
```

체크 포인트:
- `MAE steering < 0.3` 정도 — 너무 크면 학습 부족 또는 모델 망가짐
- `avg latency` 가 50 ms 이하 (50 Hz 루프 못 따라가면 차량이 늦게 반응)
- 출력 CSV 의 `pred_steering` 분포가 한쪽으로 쏠리지 않음

### 6단계: 모니터링

다른 터미널에서:
```bash
RUST_LOG=info ssh jetson@<ip> 'journalctl -f -u fsd-jetson'   # systemd 등록한 경우
```

또는 `fsd-jetson` 을 foreground 로 직접 실행해서 stdout 관찰.

## 안전 모드 진입 조건

차량 PWM 이 자동 NEUTRAL 로 가는 경우:
1. STM32 가 200 ms 동안 명령 못 받음 (Jetson hang, 시리얼 끊김 등)
2. 명령에 `estop=true` 가 들어옴
3. CRC 체크섬 실패가 연속

LED2(safe-mode 인디케이터) 가 켜진 상태면 차량은 안 움직입니다.

## 성능 튜닝

- **FP16** (`trtexec --fp16`): 속도 ↑↑, 정확도 거의 동일
- **INT8** (calibration dataset 필요): 속도 ↑↑↑, 정확도 약간 ↓
- **워크스페이스 메모리**: `trtexec --workspace=2048` 등으로 조정
- **batch=1 고정**: 단일 프레임 추론이라 dynamic batch 필요 없음

## 첫 주행 권장 절차

1. **차체를 들어 올린 상태**로 실행 — 바퀴는 돌지만 이동 안 함
2. 카메라 앞에서 사람이 시야 좌/우로 이동, steering 명령 변화 확인
3. 시뮬 OK 면 평지에서 5 m 직진 시도, 문제 없으면 코너 시도
4. 문제 발견 즉시 비상 정지 → 데이터 추가 수집 → 재학습

## 종료 / 정리

```bash
# Ctrl-C 로 정상 종료 (NEUTRAL 송신 후 close)
# 또는
pkill -SIGINT fsd-jetson
```

펌웨어는 자동으로 200 ms watchdog 발동 후 NEUTRAL.
