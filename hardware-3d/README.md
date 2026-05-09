# hardware-3d — 차체 마운트/커버 3D 모델

OpenSCAD 매개변수 설계 + STL 빌드 스크립트. **출력해서 HSP 94118 차체에 바로 장착.**

## 부품 한눈에

| 파일 | 용도 | 인쇄 시간 (대략) | 권장 재질 |
|---|---|---|---|
| `top_deck.scad` | 메인 데크 (모든 전자부품 베이스) | 6~8h | PLA 또는 PETG |
| `camera_mast.scad` | 스테레오 카메라 마스트 (80mm baseline, ±15° 틸트) | 3~4h | **PETG** (변형 적음) |
| `jetson_tray.scad` | Jetson 4 standoff | 30분 | TPU (진동 흡수) 또는 PLA |
| `stm32_clip.scad` | NUCLEO standoff + USB 스트레인 릴리프 | 30분 | PLA |
| `cover_shell.scad` | 풀 커스텀 커버 (RC 바디 대체) | 8~10h | PLA (얇음) |
| `assembly.scad` | 전체 조립 미리보기 (출력 X) | — | — |
| `params.scad` | 모든 치수 단일 진실 원천 | — | — |

총 출력 시간 약 18~22시간 (한 번에 다 안 하고 나눠서). 베드 사이즈 **220×220 이상** 필요 (커버가 큼).

## 빠른 시작

### 1. OpenSCAD 설치

- Windows: https://openscad.org/downloads.html
- macOS: `brew install openscad`
- Ubuntu: `sudo apt install openscad`

### 2. 차체 측정 → params.scad 보정

`params.scad` 의 4개 값만 자기 차체에 맞춰 측정 후 갱신:

```scad
chassis_post_spacing_x = 215;   // 자기 차체 앞뒤 body post 간 거리
chassis_post_spacing_y = 170;   // 좌우 간 거리
chassis_post_diameter  = 6;     // body post 굵기 (보통 6mm)
chassis_post_height    = 80;    // chassis 윗면 ~ post 끝
```

캘리퍼로 측정 권장. 1mm 차이도 mounting foot 가 안 맞을 수 있음.

### 3. 미리보기 (선택)

```bash
openscad assembly.scad
```

GUI 에서 deck/마스트/Jetson/NUCLEO 가 어떻게 배치되는지 확인. 충돌 있으면 `params.scad` 의 `jetson_offset_x` / `nucleo_offset_x` 조정.

### 4. STL 출력

```powershell
# Windows
pwsh hardware-3d/build.ps1
```

```bash
# Linux/macOS
./hardware-3d/build.sh
```

`hardware-3d/stl/*.stl` 5개가 생김. Cura/PrusaSlicer/Bambu Studio 등에 import.

특정 부품만:
```bash
./hardware-3d/build.sh top_deck
```

## 슬라이싱 가이드

| 부품 | layer | infill | top/bottom | support |
|---|---|---|---|---|
| `top_deck` | 0.2mm | 30% | 4 | X |
| `camera_mast` | 0.2mm | 35% | 5 (강성 중요) | 베이스 부분만 |
| `jetson_tray` / `stm32_clip` | 0.2mm | 25% | 3 | X |
| `cover_shell` | 0.3mm | 12% | 3 | X (오버행 30°↓ 제한 디자인) |

**주의**: camera_mast 는 실차 진동에서 카메라 정렬이 흐트러지면 SLAM/추론 정확도가 떨어짐 → **PETG** + infill 35% 권장. PLA 도 가능하지만 햇볕에 약함.

## 조립 순서

1. **STM32 NUCLEO 를 `top_deck` 에 장착** (M3×8 4개, `stm32_clip` standoff 사용)
2. **Jetson Orin Nano 를 `top_deck` 에 장착** (M3×10 4개, `jetson_tray` standoff 사용)
3. **카메라 PCB 를 `camera_mast` 에 장착** (M2×6 8개, IMX219 PCB 와 함께)
4. **`camera_mast` 를 `top_deck` 앞쪽에 볼팅** (M3×8 2개)
5. **deck 어셈블리를 차체 body post 4개에 끼움** (foot_clearance 0.4mm 가 PLA 수축 보정)
6. **body clip 핀 4개로 deck 잠금** (수평 슬롯에 끼움)
7. **CSI 리본 케이블 연결 → Jetson, USART3 연결 → STM32**
8. **`cover_shell` 을 deck 위에 씌움** (측면 4곳 body clip 으로 잠금)

## 매개변수 변경 시나리오

| 바꿀 것 | 어떤 변수 | 영향 |
|---|---|---|
| baseline 60mm로 컴팩트 | `camera_baseline = 60` | mast.scad 만 재출력 |
| Jetson 위치 더 앞쪽 | `jetson_offset_x = 35` | top_deck 만 재출력 |
| 차체 다른 모델 | `chassis_post_spacing_*` | top_deck + cover_shell 재출력 |
| 카메라 틸트 | `camera_tilt_deg = 12` | camera_mast 만 재출력 |
| 더 두꺼운 deck (강성 ↑) | `deck_thickness = 6` | top_deck (재료/시간 ↑) |

## 알려진 한계 / TODO

- **HSP 94118 body post 정확한 spacing 은 차체 버전마다 다름.** 본 디폴트(215×170)는 일반적 SCT 추정값. **반드시 본인 차체 측정.**
- NUCLEO-H753ZI 마운트 홀 4 모서리만 사용 (실제 보드는 6홀이지만 4개로 충분).
- 카메라 마스트 baseline 80mm 는 출력 가능 폭 이내. 120mm 로 키우면 베드 폭 체크 필요.
- 커버는 방진/방수 X. 실내/마른 야외 가정.
- **충돌 검사는 assembly.scad 가 placeholder 모양만 보여줌.** 실제 부품 크기는 BOM 데이터시트로 한 번 더 확인.

## 관련 문서

- [hardware.md](../docs/hardware.md) — 핀 매핑, BOM
- [hardware-setup-checklist.md](../docs/hardware-setup-checklist.md) — Phase 0~10 조립 절차
