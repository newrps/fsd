# resume-after-parts — 부품 도착 후 재개 가이드

부품 (수-암 점퍼와이어 / 빵판 / 멀티미터 / 저항) 도착 후 즉시 따라갈 수 있는 단계별 절차. 각 단계: **결선** → **명령** → **예상 결과** → 트러블슈팅 포인터.

## 0. 현재까지 검증된 것 (2026-05-10 기준)

- ✅ Jetson Orin Nano Super 8GB + JetPack 6.2.2 셋업
- ✅ NUCLEO-H753ZI 펌웨어 플래시 + 9-task 구조
- ✅ ST-LINK USB-VCP 양방향 시리얼 통신 (USART3 PD8/PD9 + 115200 baud)
- ✅ Embassy task polling (sync 0.7→0.8 정합)
- ✅ ml-py 파이프라인 (smoke / compare_archs / pytest / drive_sim 모두 통과)

## 0.1 비활성화 상태 (부품 결선 후 활성화)

| task | 비활성 이유 | 활성화 단계 |
|---|---|---|
| `rc_capture_steering/throttle` (PA0/PA1) | RC 수신기 미결선 → floating noise IRQ 폭주 | 단계 3 |
| `encoder_task` (PA2) | 엔코더 미결선 (동상) | 단계 4 |
| `battery_task` (ADC1 PC0) | 4:1 전압 분배기 미결선 | 단계 5 |

## 1. NUCLEO ↔ 서보/ESC 결선

### 결선 (수-암 점퍼와이어 사용)

**NUCLEO 측 — CN7 (보드 앞면 위쪽 좌측, Ethernet 잭 옆 19-pin Zio 헤더)**

| 점퍼와이어 끝 (수) | NUCLEO CN7 핀 | 라벨 | 신호 |
|---|---|---|---|
| 서보 신호선 (흰색/노랑) | pin 12 | `D12` | PA6 → 서보 PWM (TIM3_CH1) |
| ESC 신호선 (흰색/노랑) | pin 14 | `D11` | PA7 → ESC PWM (TIM3_CH2) |
| GND | pin 8 | `GND` | 공통 그라운드 |

> CN7 pin 1 위치: 보드 가장자리 끝 쪽. 1, 3, 5, 7, **8 = GND**, 9, 10, 11, **12 = D12**, 13, **14 = D11** (위쪽 줄/아래쪽 줄 번갈아).

**서보/ESC 측 — 3핀 JR 커넥터**

| 색상 | 연결 |
|---|---|
| 흰색/노랑 (신호) | 점퍼와이어 수 → NUCLEO CN7 |
| 빨강 (5V) | ❌ NUCLEO 연결 X — ESC BEC 5V (ESC 가 자체적으로 서보에 5V 공급) |
| 검정 (GND) | NUCLEO GND 와도 공통 (점퍼와이어 1개로 양쪽 검정 묶기) |

### 명령

```bash
# Jetson 에서 (USB Jetson 포트 연결)
ls /dev/ttyACM0
fsd-jetson serve --serial /dev/ttyACM0 --baud 115200
```

### 예상 결과

```
INFO  fsd-jetson starting serial=/dev/ttyACM0 baud=115200 cli.cmd=Serve
INFO  tlm t=Telemetry { seq: ..., last_applied_seq: ..., safe_mode: false, ... }
```

- ESC 비프음 (arming 완료 신호) 들림
- last_applied_seq 가 시간에 따라 증가 (Jetson cmd 처리 OK)
- safe_mode: false

### 안전 절차 (첫 시동)

1. **차량 바퀴 띄워두기** (책상 가장자리 / 받침대) — 명령 오류 시 튀어나가지 않게
2. NUCLEO USB 연결 → 3초 ESC arming → LD2 켜졌다 꺼짐
3. 모터 안 도는지 확인 (NEUTRAL cmd)
4. 작은 throttle 값 송신 (예: 0.1) → 모터 회전 확인 (별도 cmd 송신 도구 필요)

### 트러블슈팅
- NUCLEO 측 핀 못 찾음 → [firmware.md](firmware.md) 의 핀 매핑 표
- 텔레메트리 0 byte → [troubleshooting.md](troubleshooting.md) "EXTI / chip halt / VCP" 항목

---

## 2. 차량 bench 테스트

서보/ESC 결선 후 cmd 송신 시 실제 동작 확인. **차량 바퀴 띄운 상태로** 진행.

### 명령 (별도 cmd 송신 도구 필요)

`fsd-jetson` 의 `record --input gamepad` 또는 `drive` 모드 — 또는 임시로 `cmd_test.py` 스크립트 작성해 다양한 steering/throttle 값 송신.

### 예상 결과

- steering -1.0 → 서보 최대 좌
- steering +1.0 → 서보 최대 우
- throttle +0.2 → 모터 천천히 정방향 회전
- throttle -0.2 → 모터 후진 (ESC 종류에 따라 brake 모드 거쳐야 함)

### 안 되면

- 서보 / 모터 안 움직임 → ESC arming 실패 가능. ESC 종류별 시퀀스 다름 (대부분 1500µs 3초 OK)
- 방향 반대 → `firmware/src/main.rs` 의 `pwm_task` 에서 채널 swap 또는 `fsd_protocol::pwm` 의 normalized→pulse 매핑 점검

---

## 3. RC 수신기 결선 + EXTI 활성화

### 결선

**NUCLEO 측 — CN10 또는 morpho 어딘가 (PA0/PA1 위치)**

| 점퍼와이어 (수) | NUCLEO 핀 | 신호 |
|---|---|---|
| RC 수신기 ch1 (조향) 신호 | PA0 | EXTI0 입력 |
| RC 수신기 ch2 (스로틀) 신호 | PA1 | EXTI1 입력 |
| RC 수신기 GND | NUCLEO GND | 공통 |
| RC 수신기 5V (있다면) | ESC BEC 또는 별도 5V (NUCLEO 와는 별개) | - |

> PA0 / PA1 의 정확한 CN10 또는 morpho 핀 번호 — 데이터시트 Table 21/22 다시 확인 필요. 멀티미터 / 사진으로 검증.

### 펌웨어 변경 — `firmware/src/main.rs`

```rust
// 1. bind_interrupts! 에 EXTI 추가
bind_interrupts!(struct Irqs {
    USART3 => usart::BufferedInterruptHandler<peripherals::USART3>;
    EXTI0  => exti::InterruptHandler<irq_t::EXTI0>;
    EXTI1  => exti::InterruptHandler<irq_t::EXTI1>;
    EXTI2  => exti::InterruptHandler<irq_t::EXTI2>;
});

// 2. main() 의 commented 블록 활성화 (PA2 는 4단계에서)
let rc_steer_pin = ExtiInput::new(p.PA0, p.EXTI0, Pull::Down, Irqs);
let rc_thr_pin = ExtiInput::new(p.PA1, p.EXTI1, Pull::Down, Irqs);
spawner.spawn(rc_capture_steering(rc_steer_pin).unwrap());
spawner.spawn(rc_capture_throttle(rc_thr_pin).unwrap());

// 3. 위에서 let _ = (p.PA0, p.EXTI0, p.PA1, p.EXTI1, ...); 줄 제거
```

### 예상 결과

```
tlm Telemetry { ..., rc_steering: 0.05, rc_throttle: -0.12, rc_present: true, ... }
```

- RC 송신기 켜고 조작하면 rc_steering / rc_throttle 값이 -1.0 ~ +1.0 범위로 변동
- rc_present: true (양 채널 100ms 내 수신 시)

### 트러블슈팅

- spawn 후 polling 멈춤 / heartbeat tick 안 찍힘 → RC 수신기 결선 / 5V 전원 / 풀 저항 검토. [troubleshooting.md](troubleshooting.md) "EXTI 입력 task spawn 후 polling 멈춤" 참고
- rc_present: false → 100ms 내 양 채널 안 들어옴. RC 수신기 power on 확인, 송신기 binding 확인
- rc_steering 이 800µs 이하 또는 2200µs 이상 → 노이즈 또는 잘못된 채널 매핑

---

## 4. 휠 엔코더 결선 + encoder_task 활성화

### 결선

**NUCLEO 측 — PA2 (EXTI2)**

| 점퍼와이어 (수) | NUCLEO 핀 | 신호 |
|---|---|---|
| 엔코더 신호 (rising edge 출력) | PA2 | EXTI2 입력 |
| 엔코더 GND | NUCLEO GND | - |
| 엔코더 VCC | 별도 3.3V/5V (엔코더 사양에 따라) | - |

> 자기 / 광 엔코더 모두 가능. quadrature 인 경우 한 채널만 사용 = 단방향 카운트.

### 펌웨어 변경

```rust
let encoder_pin = ExtiInput::new(p.PA2, p.EXTI2, Pull::Up, Irqs);
spawner.spawn(encoder_task(encoder_pin).unwrap());
```

### 예상 결과

```
tlm Telemetry { ..., encoder_ticks: 142, ... }
```

- 차량 바퀴 회전 시 encoder_ticks 가 1씩 증가 (rising edge 마다)
- 거리 환산: ticks × (1 / pulses_per_revolution) × wheel_circumference

---

## 5. 배터리 분배기 + battery_task 활성화

### 결선 (빵판에 회로 구성)

**4:1 전압 분배기** — 11.1V LiPo → 2.78V → ADC1 PC0 (3.3V 기준 안전)

```
LiPo (+) ───┬─── R1 (30kΩ) ────┬──── ADC1 PC0
            │                  │
            │                  R2 (10kΩ)
            │                  │
            └─── ADC1 GND ─────┴────  GND
            (=NUCLEO GND, 차량 GND 와 공통)
```

> R1 = 30kΩ, R2 = 10kΩ → 분배 비율 = 10/(30+10) = 1/4. 11.1V × 1/4 = 2.78V.

| 빵판 결선 | 연결 |
|---|---|
| LiPo (+) → R1 한쪽 | 점퍼와이어 (LiPo balance 단자 또는 XT60 (+)에서 분기) |
| R1 다른쪽 ↔ R2 한쪽 | 같은 빵판 줄 (전압 분배 노드) |
| R2 다른쪽 → GND | NUCLEO GND |
| 분배 노드 (R1↔R2 사이) → NUCLEO PC0 | ADC 입력 |

⚠️ **주의**: LiPo 직결 시 회로 잘못되면 NUCLEO 즉사. 멀티미터로 분배 노드 전압이 2.5~3.0V 범위 인지 측정 후 NUCLEO 에 연결.

### 펌웨어 변경

```rust
spawner.spawn(battery_task(p.ADC1, p.PC0).unwrap());
// 위의 let _ = (p.ADC1, p.PC0); 제거
```

### 예상 결과

```
tlm Telemetry { ..., battery_v: 11.34, ... }
```

- 11.1V LiPo (3.7V × 3 셀, 만충 12.6V) 범위 표시
- 잘못된 값 (NaN, 0, 비현실적) → 분배기 회로 검토

---

## 6. CSI 카메라 결선 + record 모드

### 결선

- IMX219 카메라 모듈 × 2 (스테레오) → Jetson Orin Nano 의 J1, J3 CSI 커넥터
- 리본 케이블 방향 주의 — 금속 면이 보드 위쪽
- 카메라 마운트: `hardware-3d/camera_mast.scad` STL 출력

### 명령

```bash
fsd-jetson record \
    --serial /dev/ttyACM0 --baud 115200 \
    --out recordings/run01 --fps 30 --input gamepad
```

### 예상 결과

- `recordings/run01/manifest.jsonl` + `cam0/*.jpg` + `cam1/*.jpg` 생성
- 사용자가 게임패드 (또는 RC 송신기) 로 운전 → 명령 + 카메라 동기화 저장

---

## 7. ml-py 모델 학습 → ONNX export → drive 모드

### PC 또는 Jetson 에서 학습

```bash
cd ml-py
.venv/bin/python train.py --manifest recordings/run01/manifest.jsonl --out ckpts --epochs 30 --arch tiny
.venv/bin/python export_onnx.py --ckpt ckpts/best.pt --out model.onnx --opset 17
```

### Jetson 으로 모델 복사 + drive 모드 실행

```bash
scp model.onnx newrps@192.168.123.178:~/fsd/
ssh newrps@192.168.123.178
cd ~/fsd
fsd-jetson drive --serial /dev/ttyACM0 --baud 115200 --model model.onnx
```

### 예상 결과

- 카메라 영상 → tiny 모델 추론 → steering/throttle cmd → STM32 → 차량 자율 주행
- 추론 latency ~150µs (PC 측정 기준, Jetson 은 비슷하거나 더 빠름)

### 안전

- 첫 자율주행 테스트는 **저속 (max throttle 0.1~0.2)** + **사람 손 닿는 거리** + **emergency stop** (RC 송신기 또는 게임패드 estop) 준비

---

## 트러블슈팅 빠른 참조

| 증상 | 어디 보면 되나 |
|---|---|
| 펌웨어 안 깜빡임 / 통신 안 됨 | [troubleshooting.md](troubleshooting.md) |
| 펌웨어 코드 변경 시 빌드 에러 | [firmware.md](firmware.md) |
| EXTI 활성화 후 polling 멈춤 | [troubleshooting.md](troubleshooting.md) "EXTI 입력 task" |
| 점퍼와이어 핀 위치 헷갈림 | UM2407 (NUCLEO-H753ZI 데이터시트) Table 18~22 |
| ml-py 빌드 / pytest 실패 | `ml-py/README.md` |
| Jetson SSH 접속 안 됨 | `C:\Users\newrp\Documents\Jetson\STATUS_AND_NEXT_STEPS.md` |

## 메모

- USB power-cycle: probe-rs run 종료 시 chip halt → USB 분리/재연결로 부팅
- ST-LINK VCP path 가 default — 점퍼와이어 직결은 SB 변경 필요해서 권장 안 함 (인두 작업 필요)
- 첫 차량 동작 시 항상 바퀴 띄우기, RC 송신기로 emergency stop 가능 상태 유지
