# firmware — STM32 H753ZI 펌웨어

`firmware/` 디렉터리. Embassy 0.6 기반 `no_std` Rust.

## 한 줄 요약

NUCLEO-H753ZI 가 RC카의 모터/서보를 직접 제어하고, Jetson 과 시리얼 통신으로 명령 받고/상태 보고하는 실시간 컨트롤러.

## 8가지 기능 한눈에

| # | 기능 | 핀 | 비고 |
|---|---|---|---|
| 1 | PWM 출력 (조향 서보) | PA6 (TIM3 CH1, 50 Hz) | 1000–2000 µs 펄스 |
| 2 | PWM 출력 (ESC) | PA7 (TIM3 CH2, 50 Hz) | 부팅 시 3초 arming |
| 3 | UART 명령 수신 (Jetson → STM32) | PD9 RX (USART3) | COBS+postcard+CRC, 921600 8N1 |
| 4 | UART 텔레메트리 (STM32 → Jetson) | PD8 TX (USART3) | 50 Hz 송신 |
| 5 | RC 수신기 입력 (조향) | PA0 (EXTI0) | µs 정밀 펄스폭 |
| 6 | RC 수신기 입력 (스로틀) | PA1 (EXTI1) | 동상 |
| 7 | 휠 엔코더 카운터 | PA2 (EXTI2) | rising edge 마다 +1 |
| 8 | 배터리 전압 ADC | PC0 (ADC1) | 4:1 분배기 가정 |

추가 GPIO: PB0 (LD1 heartbeat), PE1 (LD2 safe-mode).

## 빌드

```bash
cd firmware
cargo build --release
# 산출물: target/thumbv7em-none-eabihf/release/fsd-firmware (ELF)
```

## 플래시 + RTT 로그

NUCLEO-H753ZI USB 연결 후:

```bash
cargo run --release
```

`.cargo/config.toml` 의 runner 가 `probe-rs run --chip STM32H753ZITx` 를 호출 — 자동으로:
1. ELF → 칩에 플래시
2. defmt RTT 로 stdout 에 로그 표시
3. Ctrl-C 로 종료

## 단독 플래시 (RTT 없이)

```bash
probe-rs download --chip STM32H753ZITx target/thumbv7em-none-eabihf/release/fsd-firmware
probe-rs reset --chip STM32H753ZITx
```

## 플래시 검증 (LED + RTT)

플래시 성공 시 부팅 직후 RTT 출력:
```
[INFO ] fsd-firmware starting on STM32H753ZI
[INFO ] ADC frequency set to 32000000 Hz
[INFO ] PWM max_duty = ...
[INFO ] ESC arming: 3000ms neutral pulse
[INFO ] ESC armed, accepting commands
```

LED 진단표:
| LED | 상태 | 의미 |
|---|---|---|
| LD1 (녹색, PB0) | **0.5초 토글 (1초 주기 깜빡)** | heartbeat — 펌웨어 살아있음 |
| LD2 (PE1) | 부팅 후 3초 켜졌다 꺼짐 | ESC arming → 명령 수신 대기 |
| LD2 | 3초 후에도 계속 켜짐 | safe-mode (명령 미수신 watchdog 또는 estop) |
| LD2 | 꺼짐 | 명령 처리 중, 정상 동작 |
| LD1 안 깜빡 | — | 펌웨어 부팅 실패 (RTT 로 확인) |

ST-Link USB 분리 후 외부 5V 인가해도 동일 동작 (펌웨어는 플래시 메모리에 영구 저장).

## 동작 요약

| 태스크 | 주기 | 역할 |
|---|---|---|
| `heartbeat` | 500 ms | LED1 토글 (보드 살아있나 확인) |
| `uart_rx_task` | 즉시 | 0x00 delimiter 기준 프레임 분리 → CRC 검증 → `LAST_CMD` 갱신 |
| `pwm_task` | 20 ms (50 Hz) | `LAST_CMD` 읽어 PWM duty 계산 + 출력. 200 ms 미수신 시 NEUTRAL 강제 |
| `telemetry_task` | 20 ms (50 Hz) | `Telemetry` 프레임 송신 (seq, last_applied_seq, millis, safe_mode) |
| `safe_indicator` | 변화 시 | LED2 (safe-mode 표시) |
| `rc_capture_steering` | 펄스마다 | PA0 ExtiInput rising→falling 측정, 정규화해서 `RC_INPUT` 갱신 |
| `rc_capture_throttle` | 펄스마다 | PA1 동상 (TIM2 µs tick 으로 1 µs 정밀도) |
| `encoder_task` | rising edge 마다 | PA2 EXTI2 — atomic 카운터 ENCODER_TICKS 증가 |
| `battery_task` | 100 ms | ADC1+PC0 → 16-bit raw → 4:1 분배기 보정 → mV → BATTERY_MV |

## RC 입력 캡처

- 핀 PA0/PA1 의 rising→falling 시간차를 `Instant::now()` 로 측정 (tick-hz=1 MHz, 1 µs 정밀도)
- EXTI 0/1 은 전용 NVIC IRQ — 5–9 공유 IRQ 회피 목적 (binding 단순화)
- 800–2200 µs 범위 밖이면 노이즈로 간주, 무시
- 정규화: `(pulse_us - 1500) / 500` → -1.0..+1.0 (1500 µs = 중립)
- 양 채널 값이 100 ms 내에 둘 다 갱신되면 `rc_present = true` 로 텔레메트리에 표시
- RC 수신기 미연결이거나 송신기 OFF 시 자동으로 `rc_present = false` (자율주행/게임패드 우선)

## ESC arming (시작 시퀀스)

대부분 RC ESC 는 부팅 시 일정 시간 중립 펄스(1500 µs)를 봐야 정상 arming.
- 부팅 직후 **3 초간 강제로 1500 µs 송출** + safe-mode LED 켜짐
- 이 동안 들어온 명령은 무시 (NEUTRAL 유지)
- 3 초 경과 후 LED off + 명령 처리 시작 + RTT 로그 `ESC armed`
- `pwm_task` 의 `arming_secs` 상수로 시간 조정 가능

**실차 첫 시동 절차**:
1. 차량 전원 인가 (Jetson + STM32 + ESC)
2. STM32 LED2(safe) 가 3 초간 켜졌다 꺼지는지 확인
3. 꺼진 후에야 차량 명령에 반응

이 시퀀스 없이 부팅하면 ESC 가 arming 실패 → 명령 줘도 모터 동작 안 함 (또는 일관되지 않은 동작).

## 안전 동작

- **Watchdog**: `LAST_CMD` 가 200 ms 이상 갱신 안 되면 PWM 자동 중립
- **estop 비트**: `DriveCommand.estop = true` 면 즉시 중립 + safe-mode 진입
- **CRC 실패 / postcard 디코드 실패**: 해당 프레임 drop, NEUTRAL 유지
- **시작 직후**: 위 ESC arming 으로 3 초간 NEUTRAL. 그 후엔 `LAST_CMD` default = NEUTRAL

## 로그 레벨 변경

`.cargo/config.toml` 의 `DEFMT_LOG=info` 를 `debug` / `trace` 로 변경. 또는:

```bash
DEFMT_LOG=debug cargo run --release
```

## 핀 변경 가이드

`firmware/src/main.rs` 상단의 핀 매핑 코멘트 + 실제 코드에서 `p.PA6`, `p.PA7`, `p.PD8`, `p.PD9` 부분만 바꾸면 됨. 변경 시 `docs/hardware.md` 표도 같이 갱신.

## RCC (클럭) 설정 메모

```rust
config.rcc.pll1 = Some(Pll {  // 240 MHz sysclk
    source: HSI, prediv: DIV4, mul: MUL30, divp: DIV2, ...
});
config.rcc.sys = Sysclk::PLL1_P;          // 240 MHz
config.rcc.mux.adcsel = mux::Adcsel::PER; // ADC 클럭을 PER(=HSI 64MHz)로
```

⚠️ **ADC mux 명시 필수**: embassy-stm32 0.6 의 ADC1 기본 클럭이 `pll2_p` 인데 우리는 PLL2 미사용. 그대로 두면 부팅 직후 패닉:
```
panicked: peripheral 'ADC1' is configured to use the 'pll2_p' clock, which is not running
```
→ `adcsel = PER` 로 우회 (HSI 64 MHz, ADC 동작 32 MHz, 80 MHz 스펙 만족).

## 장애 시

- LED1 안 깜빡임: 클럭/전원 문제. 또는 `embassy_stm32::init` 실패 (가장 흔히 RCC 설정)
- LED1 깜빡이지만 명령 무반응: UART 배선/baud 확인. 921600 8N1
- PWM 출력만 이상: `pwm_task` 의 `max_duty` 로그 확인. TIM3 클럭 확인
- 패닉 `pll2_p clock not running`: 위 ADC mux 메모 참고
- 자세한 트러블슈팅: [troubleshooting.md](troubleshooting.md)
