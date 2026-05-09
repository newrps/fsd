# hardware — 부품 / 핀 매핑 / 배선

## BOM (Bill of Materials)

| 부품 | 모델 | 역할 |
|---|---|---|
| 차체 | HSP 94118 (1/10) | 서보 + ESC + 모터 + 휠 |
| 메인 보드 | NVIDIA Jetson Orin Nano Super Dev Kit | AI/Vision 처리 |
| MCU | STM32 NUCLEO-H753ZI | PWM 제어, 안전 watchdog |
| 카메라 | IMX219 8MP 120° (VLT-JN006) × 2 | 듀얼/스테레오 비전 |
| 배터리 | LiPo 7.4V (차체용) + Jetson 별도 전원 | |
| BEC / 전원 | ESC 내장 BEC 또는 별도 5V→Jetson | Jetson 5V/4A 권장 |

## STM32 핀 매핑

`firmware/src/main.rs` 가 단일 진실 원천. 변경 시 본 문서도 갱신.

| 기능 | 핀 | 페리페럴 | 비고 |
|---|---|---|---|
| **UART → Jetson TX** | PD8 | USART3 TX | NUCLEO ST-LINK VCP 라인. 실차에서는 PB10 권장 |
| **UART ← Jetson RX** | PD9 | USART3 RX | 동상. 실차 시 PB11 |
| **서보 PWM (조향)** | PA6 | TIM3 CH1 | 50 Hz, 1000–2000 µs |
| **ESC PWM (스로틀)** | PA7 | TIM3 CH2 | 50 Hz, 1000–2000 µs |
| **RC in (조향)** | PA0 | EXTI0 | RC 수신기 채널 1 출력. EXTI0 전용 NVIC IRQ (5-9 공유 IRQ 회피) |
| **RC in (스로틀)** | PA1 | EXTI1 | RC 수신기 채널 2 출력. EXTI1 전용 NVIC IRQ |
| **휠 엔코더** | PA2 | EXTI2 | rising edge 마다 카운터 +1. 단방향 (방향 구분은 quadrature 추가 필요) |
| **배터리 ADC** | PC0 | ADC1 | 외부 4:1 전압 분배기 가정 (11.1V LiPo → 2.78V → ADC). 100ms 마다 측정 |
| **LED1 (heartbeat)** | PB0 | GPIO out | NUCLEO 보드 LED1 |
| **LED2 (safe-mode)** | PE1 | GPIO out | NUCLEO 보드 LED2 |

## Jetson ↔ STM32 시리얼

- 보드레이트: **921 600** 8N1
- 프레이밍: COBS (0x00 = delimiter)
- 페이로드: postcard + CRC16-IBM (`fsd-protocol` crate)
- 포트:
  - Jetson 측: `/dev/ttyACM0`(ST-LINK VCP) 또는 `/dev/ttyTHS1`(40-pin GPIO UART, 실차 권장)
  - STM32 측: USART3 (위 핀 표 참고)

### 실차 권장 배선 (ST-LINK 미경유)

```
Jetson 40-pin GPIO          STM32 (PB10/PB11)
   pin 8 (UART TX)  ──────▶ PB11 (USART3 RX)
   pin 10 (UART RX) ◀────── PB10 (USART3 TX)
   pin 6 (GND)      ─────── GND
```

**전원 GND 공통 필수.** 신호선엔 100Ω 정도의 직렬 저항 + ESD 다이오드 권장.

## RC 수신기 배선 (선택 사항)

기존 RC 송신기/수신기로 운전하고 싶을 때만. 게임패드만 쓸 거면 생략.

```
RC 수신기 채널 1 (조향) ──▶ STM32 PA0   (5 V → 3.3 V level shift 또는 직렬 1 kΩ + 클램프 다이오드 권장)
RC 수신기 채널 2 (스로틀)──▶ STM32 PA1
RC 수신기 GND          ─── STM32 GND
RC 수신기 5 V          ─── (별도 BEC, STM32 3.3 V 와는 분리)
```

**중요**: RC 수신기는 보통 5 V 신호. STM32 3.3 V 핀 입력은 5 V tolerant 인 핀과 아닌 핀이 섞여 있음. PB6/PB7 은 5 V tolerant 이지만 안전을 위해 1 kΩ 직렬 + 3.3 V Zener 클램프 권장.

펌웨어가 50 Hz, 1000–2000 µs 펄스를 µs 정밀도로 캡처해 텔레메트리에 포함시킨다.

## RC 신호 사양 (서보 / ESC / RC 수신기 입력)

| 값 | 펄스폭 | 정규화 입력 |
|---|---|---|
| 최대 좌 / 후진 | 1000 µs | -1.0 |
| 중립 | 1500 µs | 0.0 |
| 최대 우 / 전진 | 2000 µs | +1.0 |
| 주기 | 20 000 µs (50 Hz) | |

## 카메라 (IMX219)

- Jetson Orin Nano Super 의 CSI-2 포트 두 개에 각각 연결
- 둘 다 사용 시 GStreamer 에서 `sensor-id=0` / `sensor-id=1`
- 광각 120° 렌즈는 distortion 큼 → 추후 calibration 필요 (다른 문서)

## 배선 안전

- LiPo 메인 전원과 Jetson 전원은 **분리** 권장 (모터 inrush 로 Jetson reboot 위험)
- ESC BEC 가 5V/3A 만 줄 경우 Jetson 5V/4A 요구 못 맞춤 — 별도 BEC 또는 step-down
- 비상 정지 스위치 물리적으로 ESC 전원 차단하는 위치에 두기
