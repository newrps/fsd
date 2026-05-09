# data-collection — 학습 데이터 수집

모방학습은 데이터 품질이 모델 품질입니다. 모델이 아무리 좋아도 데이터가 편향되면 똑같이 편향됩니다.

## 입력 소스

`fsd-jetson record --input <source>` 로 두 가지 사람 입력 지원:

| `--input` | 설명 | 빌드 요건 |
|---|---|---|
| `rc` | RC 송신기로 운전. 수신기 → STM32 펌웨어가 PWM 캡처 → 텔레메트리로 jetson 전달 | 펌웨어만 필요 (PB6/PB7 배선) |
| `gamepad` | Xbox/PS USB 컨트롤러를 Jetson 에 연결 | `--features gamepad` |
| `auto` (기본) | gamepad init 되면 gamepad, 안 되면 RC 로 fallback | gamepad 우선이라 `--features gamepad` 권장 |

게임패드 매핑 (Xbox 기준):
- 좌측 스틱 X = 조향
- RT (우 트리거) = 전진
- LT (좌 트리거) = 후진
- A (South) 버튼 = estop

RC 와 gamepad 동시 연결 시 `--input` 플래그로 명시 선택. 사용 중 입력원 끊기면 펌웨어 200 ms watchdog 가 NEUTRAL 강제.

## 데이터 형식

### manifest.jsonl

```json
{"seq":0,"t":"2026-05-08T12:34:56.789Z","steering":-0.12,"throttle":0.45,"cam0":"cam0/00000000.jpg","cam1":"cam1/00000000.jpg"}
```

| 필드 | 타입 | 설명 |
|---|---|---|
| `seq` | u64 | 단조 증가. 동기화 검증용 |
| `t` | RFC3339 timestamp | UTC |
| `steering` | f32 | -1.0 (좌) ~ +1.0 (우) |
| `throttle` | f32 | -1.0 (후진) ~ +1.0 (전진) |
| `cam0` | path | 좌측 카메라 JPEG (manifest 기준 상대 경로 또는 절대 경로) |
| `cam1` | path? | 우측 카메라 JPEG, 없으면 null |

### 이미지

JPEG quality=85, 1280×720. 학습 시 200×66 으로 리사이즈 (PilotNet 입력).

## 권장 수집 절차

1. **시작 전**: 차체에 안전 거리 확보, 비상정지 스위치 작동 확인
2. **다양성 확보**:
   - 직선 / 좌·우 코너
   - 다른 표면 (asphalt, 매트, 카펫)
   - 다른 조명 (밝음, 그늘, 형광등)
   - 다른 속도
3. **균형**: steering 분포가 한쪽으로 쏠리지 않게. `numpy.histogram` 으로 사후 점검
4. **시간**: 처음엔 5–10분 분량 (~9000–18000 프레임 @ 30fps)으로 시작

## 분량 기준 (경험치)

| 단계 | 프레임 수 | 효과 |
|---|---|---|
| 초기 sanity | 1k–5k | 학습 파이프라인 동작 확인 |
| 1차 모델 | 10k–50k | 단일 코스에서 그럭저럭 |
| 견고한 모델 | 100k+ | 여러 환경 대응 |

## 사후 점검 (학습 시작 전 권장)

```bash
cd ml-py
python plot_distribution.py --manifest ../recordings/run-001/manifest.jsonl
# → recordings/run-001/distribution.png
```

생성되는 PNG (6 패널):
1. **steering 히스토그램** — 좌/중앙/우 분포 균형. mean 이 0 에서 멀거나 std 가 너무 작으면 ⚠ 경고
2. **throttle 히스토그램** — 동일
3. **frame interval** — 캡처 jitter (50 Hz 면 median 20ms 부근). 스파이크 있으면 동기화 문제
4. **steering over seq** — 시간 순 변화. 갑작스런 점프(>0.5) 는 보통 에러
5. **throttle over seq** — 동상
6. **steering vs throttle 산점도** — 운전 스타일 (방어적 vs 공격적)

steering 분포가 한쪽으로 쏠리면 (`mean | > 0.1`) → 코스를 반대 방향으로도 녹화하거나, 좌우 flip augmentation 만으로 부족한지 점검.

## 데이터 augmentation (학습 시)

다음 augmentation 이 자동 적용됩니다 (train subset 만, val 은 원본 그대로):

| 항목 | 강도 | 영향 |
|---|---|---|
| 좌/우 hflip + steering 부호 반전 | 50% 확률 | steering 분포 좌우 균형 + 사실상 데이터 2배 |
| 밝기 jitter | × [0.8, 1.2] | 조명 변화 robustness |
| 대비 jitter (mean 기준) | × [0.8, 1.2] | 노출 차이 robustness |
| **recovery shift + steering 보정** | 30% 확률, ±20 px | 차선 이탈 후 복귀 학습. shift 1 px 당 0.004 의 steering 보정 |

throttle 은 변형되지 않습니다 (좌우 flip 과 무관).

### 입력 정규화 (mean/std)

모델 입력은 `(x - mean) / std` 로 표준화됩니다. 정규화 layer 가 **모델 안에 포함**되어 있어:
- 추론 시 별도 정규화 코드 불필요 (jetson 측은 그냥 0..1 RGB 텐서를 모델에 입력)
- ONNX export 시 정규화도 함께 export → TensorRT 엔진 빌드 시 자동 fuse

**stats 자동 계산** (Python 경로):
- `ml-py/compute_stats.py` 가 manifest 한 번 훑어 채널별 mean/std 계산
- `train.py` 가 `manifest 디렉터리/stats.json` 없으면 자동 호출 + 저장
- 학습 시 `PilotNet(mean=..., std=...)` 생성자에 전달

```bash
# 미리 계산하기 (선택)
python compute_stats.py --manifest ../recordings/run-001/manifest.jsonl
# train.py 가 stats.json 자동 활용
```

burn 경로도 동일 정책: `PilotNetConfig::default().stats` 가 default 값(0.45/0.46/0.43, 0.22/0.22/0.22). 학습 코드에서 stats 계산해서 config 에 주입하는 부분은 TODO.

### 정책 일관성

augmentation 정책은 **두 학습 경로 모두 동일**하게 유지됩니다:
- Rust burn: `ml/src/data.rs::augment_in_place` (`DrivingBatcher::train()` 사용 시 적용)
- Python PyTorch: `ml-py/dataset.py::AugmentingDataset` (`augment=True`)

정책 변경 시 두 파일 모두 동시 갱신.

### Recovery shift 의미

차량이 차선 중앙에서 ±20 px (실세계 ~10 cm) 옆으로 벗어난 시점의 영상을 시뮬레이션해서, 그 시점에 적절한 steering 보정량을 학습 데이터에 추가한다. 이게 없으면 모델은 "차선 중앙에서 본 영상" 만 학습해서, 실차에서 한 번 차선을 벗어나면 복귀할 줄 모른다.

### 추가 후보 (TODO)

- 하단 일부 가림 (random shadow / occlusion patch)
- HSV 색공간 jitter (white balance 차이)
- gamma 조정
- burn 학습 코드에서 stats 자동 계산해서 PilotNetConfig 에 주입 (현재 default 값만 사용 중)
