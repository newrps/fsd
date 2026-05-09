# troubleshooting — 자주 마주치는 문제

문제 발견 시 본 문서에 항목 추가. 한 줄 증상 + 원인 + 해결.

## 빌드

### firmware embassy 0.6 마이그레이션 (완료, 참고용)

마이그레이션 경험 기록 — 같은 작업을 다시 할 때 참고:

| 0.2 API | 0.6 API | 메모 |
|---|---|---|
| `SimplePwm::enable(Channel::Ch1)` | `pwm.ch1().enable()` | accessor 패턴 |
| `SimplePwm::set_duty(Ch1, d)` | `pwm.ch1().set_duty_cycle(d: u32)` | u16 → u32 |
| `SimplePwm::get_max_duty() -> u32` | `pwm.ch1().max_duty_cycle() -> u32` | 채널별 |
| `PwmPin::new_ch1(pin, OutputType)` | `PwmPin::new(pin, OutputType)` | 채널은 핀 타입에서 결정 (TimerPin 트레이트) |
| `BufferedUart::new(peri, irq, rx, tx, tx_buf, rx_buf, cfg)` | `BufferedUart::new(peri, rx, tx, tx_buf, rx_buf, irq, cfg)` | irq 가 buffer 뒤로 |
| `Spawner::must_spawn(task)` | `Spawner::spawn(task.unwrap())` | task 매크로가 Result 반환 |
| `ExtiInput<'static>` | `ExtiInput<'static, Async>` | mode 제네릭 추가 |
| `ExtiInput::new(pin, exti, pull)` | `ExtiInput::new(pin, exti, pull, irq)` | IRQ binding 인자 추가 |
| `embedded_io_async` 자동 | `embedded-io-async = "0.7"` 직접 dep | embassy-stm32 0.6 은 0.7 사용 |
| `rx.read(&mut buf).await` | `embedded_io_async::Read::read(&mut rx, &mut buf).await` | private inherent `read` 가 trait shadow → UFCS |
| EXTI 9_5 같은 공유 IRQ | `bind_interrupts!{ EXTI9_5 => exti::InterruptHandler<typelevel::EXTI9_5> }` | 또는 EXTI0–4 처럼 전용 IRQ 핀 사용 권장 |

**핀 변경 권장 — RC 입력은 EXTI 0–4 (전용 IRQ) 가 binding 단순함.** 공유 IRQ (EXTI9_5, EXTI15_10) 는 typelevel marker 까지 알아내야 함.

embassy 0.6 examples: https://github.com/embassy-rs/embassy/tree/main/examples/stm32h7

### `cargo build` — embassy-stm32 / burn API 불일치 (일반)
> **증상**: type signature 가 안 맞는다는 에러.

embassy-stm32 / burn 은 마이너 버전 사이에도 API drift 가 있습니다. `Cargo.toml` 의 버전 핀 확인 + 해당 crate 의 CHANGELOG 참고.

### `cargo build` — `error: linker 'arm-none-eabi-gcc' not found`
> **증상**: 펌웨어 빌드 시.

LLD 가 기본이라 보통 안 나오지만, 특정 환경에서 발생. `firmware/.cargo/config.toml` 의 rustflags 에 `-C linker=rust-lld` 추가.

## 플래시 / RTT

### `probe-rs run` — `No probe found`
> ST-LINK 가 안 잡힘.

- USB 케이블 확인 (충전 전용 케이블 X) — Mini-USB B 는 충전 전용 비율 높음. 폰 데이터 케이블이나 SSD 케이블로 교체
- 보드 LED 진단:
  - **LD3(빨강 user) 깜빡 + LD5(노랑/빨강 5V) 켜짐** = 보드 정상, USB 데이터 라인만 문제 → 케이블 교체
  - 둘 다 안 켜짐 = 전원 미인가 → JP1=`U5V`, JP5(IDD) 점퍼 확인
- Windows: 장치관리자에서 `VID_0483&PID_374E`(ST-Link V3) 보이는지 확인. 없으면 케이블, 있는데 노란 느낌표면 ST-LINK 드라이버 설치 (st.com STSW-LINK009)
- Linux: udev rule. probe-rs 문서의 `/etc/udev/rules.d/69-probe-rs.rules` 적용 + `sudo udevadm control --reload`

### 펌웨어 부팅 직후 패닉 — `peripheral 'ADC1' is configured to use the 'pll2_p' clock, which is not running`
> **증상**: NUCLEO 에 플래시 후 RTT 첫 줄 출력 직후 panic_probe 가 hard fault 일으킴.

embassy-stm32 0.6 의 ADC1 기본 클럭 mux 가 `pll2_p` 인데 RCC 설정에 PLL2 가 없으면 발생. 해결:
```rust
// firmware/src/main.rs 의 RCC 설정 블록 끝에 추가
config.rcc.mux.adcsel = mux::Adcsel::PER;
```
PER(peripheral) 클럭 = 기본 HSI 64 MHz (이미 활성). ADC 동작 32 MHz, 80 MHz 스펙 내라 안전.
RTT 에 `[INFO] ADC frequency set to 32000000 Hz` 가 보이면 정상.

대안: PLL2 를 추가 설정해 `pll2_p` 활성화 (정밀 클럭 필요 시).

### `probe-rs run` — `target not halted` 또는 `Error: ARM error`
> 칩이 응답 없음.

- NRST 핀에 noise. 디커플링 캡 확인
- Boot0 핀이 high 로 stuck → 시스템 메모리 부팅 모드 → SWD 안 됨. Boot0=GND 확인
- 마지막 수단: `probe-rs erase --chip STM32H753ZITx` (모든 flash 지움)

### Windows 에서 `cargo build --release --features onnx` 시 ort-sys build 실패
> **증상**: `ort-sys` 의 build script 가 `ureq::ConfigBuilder::tls_config` 를 못 찾는다고 오류.

ort 2.0-rc.12 의 download-binaries build script 가 최신 ureq 3.x 의 API 변경(`tls_config` → 다른 메서드)과 호환되지 않는 알려진 버그. 해결 옵션:

1. **`cargo check` 로 우회 검증** — 컴파일 검증만 필요하면 충분
2. **WSL 또는 Linux 에서 빌드** — Jetson 배포가 결국 Linux 라 일관성 좋음
3. **수동으로 ONNX Runtime DLL 받아서 ORT_LIB_PATH 지정**:
   - https://github.com/microsoft/onnxruntime/releases 에서 onnxruntime-win-x64.zip 받기
   - 압축 해제 후: `setx ORT_LIB_PATH C:\path\to\onnxruntime\lib`
   - feature 를 `["std"]` 로만 두고 build (download-binaries 끄기)

Jetson 배포 시에는 JetPack 의 system ORT 를 `ORT_DYLIB_PATH` 로 사용 — 이 build script 이슈 안 만남.

## 시리얼

### Jetson 측 `Permission denied: /dev/ttyACM0`
```bash
sudo usermod -aG dialout $USER
# 로그아웃/재로그인 또는
newgrp dialout
```

### USART3 RX 에 garbage 수신
- 보드레이트 안 맞음 (양쪽 921600 8N1?)
- TX/RX 교차 안 됨 (Jetson TX → STM32 RX)
- GND 공통 안 잡힘
- 케이블 너무 길거나 noise — 직렬 100Ω 저항 + 짧게

## 카메라

### `gst-launch-1.0 nvarguscamerasrc` — `Could not open camera`
- CSI 케이블 분리/재접속
- `sudo systemctl restart nvargus-daemon`
- 권한: `sudo usermod -aG video $USER`

### 듀얼 카메라 동기 차이
IMX219 두 대를 완벽히 동기화하려면 hardware sync(EXTSYNC 핀)가 필요. SW 동기는 ~1 frame jitter. 학습엔 보통 충분, SLAM 엔 부족.

## ONNX / TensorRT

### `ort` — `ORT_DYLIB_PATH not set`
Jetson 에서 시스템 ORT 사용 시:
```bash
export ORT_DYLIB_PATH=/usr/lib/aarch64-linux-gnu/libonnxruntime.so
```
PC 에서는 `ort` 의 `download-binaries` feature(default) 가 자동으로 binary 다운로드.

### `trtexec` — `Unsupported ONNX node`
- opset 너무 높음 (TRT 버전이 따라가지 못함). `--opset 17` 정도로 export
- 모델에 TRT 미지원 op 있음 — 모델을 단순화하거나 plugin 작성

### TensorRT FP16 결과가 이상함
일부 layer 가 FP16 에서 underflow/overflow. `--layerPrecisions` 로 특정 layer 만 FP32 강제.

## 학습

### MSE loss 가 줄지 않음
- 데이터 정규화 잘못됨. 입력 0..1 인지 확인
- LR 너무 큼 / 작음. 1e-4 부근에서 시작
- 데이터 너무 적음 (수백 샘플로는 안 됨)

### 학습 잘 되는데 차량에서 NEUTRAL 만 나옴
- 학습 데이터가 NEUTRAL 위주 → 모델이 NEUTRAL 만 출력. 데이터 분포 점검 (data-collection.md)
- 입력 전처리가 학습/추론 사이 다름. 둘 다 200×66, 0..1, CHW, RGB 인지 확인

## 차량 동작

### LED2 (safe-mode) 항상 켜짐
- Jetson 에서 명령 안 옴 (200 ms watchdog 발동). serve 모드로 시리얼 살아있는지 확인
- 명령에 `estop=true` 들어가는 중. drive 코드 점검

### 서보 / ESC 가 진동만 함
- PWM 주파수 안 맞음 (50Hz 인지 확인)
- 펄스폭 범위 안 맞음 (1000–2000 µs). 일부 ESC 는 별도 calibration 필요
