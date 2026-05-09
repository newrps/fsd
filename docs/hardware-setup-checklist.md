# hardware setup — step-by-step 체크리스트

각 단계: **명령** → **예상 결과** → 안 되면 어디 보면 되는지. 순서대로 따라가면 됩니다.

## Phase 0 — 부품 확인

- [ ] 차체 HSP 94118 (서보 + ESC + 모터 + LiPo 배터리 + RC 수신기)
- [ ] STM32 NUCLEO-H753ZI + Mini USB 케이블
- [ ] Jetson Orin Nano Super Dev Kit + 5V/4A 어댑터 + USB-C 케이블
- [ ] IMX219 카메라 모듈 × 2 + CSI 리본 케이블 × 2
- [ ] (선택) USB 게임패드 — Xbox/PS4
- [ ] 점퍼선 (M-F, M-M 각각 5–10개)
- [ ] 멀티미터 (배선 검증용)

## Phase 1 — Jetson 부팅 (1회성, 30분~1시간)

```bash
# Jetson NX SD 이미지 굽기 (PC):
#   https://developer.nvidia.com/embedded/jetpack 에서 JetPack 6.x SDK Manager 또는
#   직접 SD 이미지 다운 → balenaEtcher 로 SD 카드에 굽기
```

- [ ] SD 카드 삽입 → HDMI 모니터 + USB 키보드/마우스 연결 → 전원 인가
- [ ] Ubuntu 첫 셋업 진행 (계정 만들기, Wi-Fi)
- [ ] `nvidia-smi` 동작 확인 (GPU 인식)
- [ ] `sudo apt update && sudo apt upgrade -y`

## Phase 2 — STM32 펌웨어 플래시 (PC, 10분)

```powershell
# PC 에서 (이미 fsd 클론돼 있다고 가정)
cd C:\git\fsd\firmware
cargo build --release
```

- [ ] STM32 NUCLEO-H753ZI 를 USB 로 PC 에 연결 (ST-LINK 가 가상 디스크로 잡힘)
- [ ] `cargo run --release` — probe-rs 가 자동 플래시 + RTT 로그 시작
- **예상**: defmt 로그 `fsd-firmware starting on STM32H753ZI` + LED1(녹) 0.5초 주기 깜빡임
- 안 되면: [troubleshooting.md](troubleshooting.md) 의 probe-rs 섹션

## Phase 3 — Jetson UART ↔ STM32 배선 (10분)

가장 처음엔 ST-LINK VCP (PD8/PD9) 로 시도 — 별도 배선 없이 USB 만으로 시리얼 동작:

```bash
# Jetson 에서
ls /dev/ttyACM*    # USB 로 STM32 연결되면 잡힘
```

- [ ] ST-LINK USB 를 Jetson 에 연결 (Jetson 의 USB 포트에)
- [ ] `/dev/ttyACM0` 또는 비슷한 디바이스 인식 확인

**또는 실차 권장 배선** (USB 분리 후):

```
Jetson 40-pin GPIO              STM32 (PB10/PB11 사용 시 펌웨어 코드 수정 필요)
   pin 8  (UART TX, /dev/ttyTHS1)  ──▶ PB11 (USART3 RX)
   pin 10 (UART RX)                ◀── PB10 (USART3 TX)
   pin 6  (GND)                   ──── GND
```

## Phase 4 — 시리얼 통신 검증 (`fsd-jetson serve`)

```bash
# Jetson
cd ~/fsd
cargo build --release -p fsd-jetson    # 첫 빌드 5–10분
./target/release/fsd-jetson --serial /dev/ttyACM0 serve
```

- **예상 로그** (50Hz 텔레메트리):
  ```
  INFO tlm Telemetry { seq: 1, last_applied_seq: 0, millis: 234, encoder_ticks: 0,
                       battery_v: NaN, safe_mode: true, rc_steering: NaN, ... }
  ```
- [ ] `safe_mode: true → false` 로 3초 안에 변하는지 (ESC arming 끝났다는 뜻)
- [ ] `seq` 가 단조 증가
- 안 되면:
  - dialout 그룹 (`sudo usermod -aG dialout $USER` + 재로그인)
  - baud 일치 (펌웨어 921600, jetson 같음)
  - GND 공통 (외부 배선 사용 시)

## Phase 5 — PWM (서보 + ESC) 배선 + 시동

```
STM32           차체
  PA6  ──▶  서보 신호선 (백색)
  PA7  ──▶  ESC 신호선 (백색)
  GND  ──── 서보/ESC GND (검정)
서보 5V       ESC 의 BEC 5V (붉은선) — STM32 와는 분리
```

- [ ] **차체를 들어올려** (바퀴 공중에) 안전 확보
- [ ] LiPo 배터리 차체에 연결 → ESC 부팅음 확인
- [ ] STM32 LED2 (PE1, 노랑) **3초간 켜짐 → 꺼짐** (ESC arming 완료)
- [ ] `serve` 실행 중인 상태로 그대로 둠 → 서보가 중립 위치, 모터 안 움직임 (정상)

```bash
# 명령 직접 보내서 서보 흔들리는지 (다른 터미널)
# 일단은 stub — 나중에 별도 CLI 필요. 지금은 record 모드로 우회.
```

## Phase 6 — 카메라 연결 + 캡처 검증

- [ ] Jetson 전원 끄고 → CSI 리본 카메라 2 개 → `cam0`/`cam1` 포트 연결 → 전원 인가
- [ ] GStreamer 단독 검증:
  ```bash
  gst-launch-1.0 nvarguscamerasrc sensor-id=0 num-buffers=1 \
    ! "video/x-raw(memory:NVMM), width=1280, height=720, framerate=30/1" \
    ! nvjpegenc ! filesink location=/tmp/cam0.jpg
  gst-launch-1.0 nvarguscamerasrc sensor-id=1 num-buffers=1 \
    ! "video/x-raw(memory:NVMM), width=1280, height=720, framerate=30/1" \
    ! nvjpegenc ! filesink location=/tmp/cam1.jpg
  ```
- [ ] `eog /tmp/cam0.jpg /tmp/cam1.jpg` 으로 둘 다 영상 확인
- 안 되면: `sudo systemctl restart nvargus-daemon`, 카메라 리본 다시 꽂기, 권한 (`sudo usermod -aG video $USER`)

## Phase 7 — 입력 소스 (RC 수신기 또는 게임패드)

### Option A: RC 수신기

```
RC 수신기 채널 1 (조향) ──▶ STM32 PA0   (1 kΩ + 3.3V Zener 클램프 권장)
RC 수신기 채널 2 (스로틀)──▶ STM32 PA1
RC 수신기 GND          ─── STM32 GND
```

- [ ] 송신기 ON → STM32 텔레메트리에 `rc_present: true` + `rc_steering`/`rc_throttle` 값
  변화 확인 (스틱 움직이면 -1.0 ~ +1.0 사이)

### Option B: USB 게임패드

```bash
sudo apt install libudev-dev
ls /dev/input/event*    # 게임패드 USB 꽂으면 추가됨
```

- [ ] `cargo build --release -p fsd-jetson --features "camera,gamepad"`
- [ ] dialout/input 그룹 권한

## Phase 8 — 첫 데이터 수집 (짧게)

```bash
./target/release/fsd-jetson --serial /dev/ttyACM0 \
    record --out recordings/run-001 --input gamepad --fps 30
```

- [ ] **차체를 다시 들어올린 채로** 시작 (안전)
- [ ] 30초 정도 손으로 흔들면서 게임패드/RC 로 운전 시뮬
- [ ] Ctrl-C 종료
- [ ] `ls recordings/run-001/` 에 manifest.jsonl + cam0/*.jpg + cam1/*.jpg
- [ ] `python plot_distribution.py --manifest recordings/run-001/manifest.jsonl`
  → distribution.png 에 steering/throttle 변화 그래프 확인

## Phase 9 — 첫 학습 + 자율주행 (사전 준비 OK 시)

```bash
cd ml-py
pip install -r requirements.txt
python train.py --manifest ../recordings/run-001/manifest.jsonl --out ckpts --epochs 30
python export_onnx.py --ckpt ckpts/best.pt --out ../model.onnx
```

- [ ] 학습 진행 중 val_loss 감소 확인
- [ ] `python replay.py --recording ../recordings/run-001 --model ../model.onnx`
  → MAE_steering < 0.3 정도면 학습 OK

```bash
# 차체를 들어올린 채로
./target/release/fsd-jetson --serial /dev/ttyACM0 \
    drive --model ../model.onnx
```

- [ ] 카메라 앞에 손 흔들면 서보가 따라 움직임 (방향성 OK 면 합격)
- [ ] OK 면 평지에 차를 내려놓고 5m 직진 시도 → 코너 시도

## Phase 10 — systemd 등록 (선택)

```bash
# Jetson 에서
sudo ./scripts/install_systemd.sh
sudo systemctl enable --now fsd-jetson
sudo journalctl -fu fsd-jetson
```

- [ ] boot 시 자동 시작
- [ ] `model.onnx`/`stereo_calib.json` 가 `/home/jetson/fsd/` 에 있어야 함

## 트러블슈팅 빠른 인덱스

| 증상 | 보세요 |
|---|---|
| probe-rs / 플래시 실패 | [troubleshooting.md#플래시--rtt](troubleshooting.md) |
| 시리얼 권한 / 안 잡힘 | [troubleshooting.md#시리얼](troubleshooting.md) |
| 카메라 안 뜸 | [troubleshooting.md#카메라](troubleshooting.md) |
| ONNX/TensorRT 오류 | [troubleshooting.md#onnx--tensorrt](troubleshooting.md) |
| ESC arming 안 됨 (LED2 안 켜짐 / 안 꺼짐) | [firmware.md#esc-arming-시작-시퀀스](firmware.md) |

## 안전 원칙

- 첫 자율주행 시도는 **반드시 차체를 들어올린 상태에서** 시작
- 평지 시도 시 사람이 닿을 수 있는 거리에서, **물리 비상정지 스위치** (배터리 분리 가능한 위치) 확보
- 다른 사람·반려동물·깨지기 쉬운 물체 없는 공간
- LiPo 배터리는 충전 시 화재 위험 — 충전백 안에서, 사람 있을 때만
