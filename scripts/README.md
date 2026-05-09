# scripts — 배포·운영 자동화

## 파일

| 파일 | 용도 |
|---|---|
| `quickstart.ps1` / `quickstart.sh` | 처음 클론 후 1-명령 ML 데모 (venv + pytest + smoke + compare + notebook_demo) |
| `deploy.sh` | 개발 PC → Jetson rsync + 원격 빌드 + (선택) systemd 재시작 |
| `fsd-jetson.service` | systemd 유닛 — boot 시 fsd-jetson drive 자동 실행 |
| `install_systemd.sh` | Jetson 위에서 systemd 유닛 설치 |

## 처음 클론한 사람: 1-명령 데모

```bash
# Linux/macOS
./scripts/quickstart.sh

# Windows
pwsh scripts/quickstart.ps1
```

자주 쓰는 옵션:

| 옵션 (sh) | 옵션 (ps1) | 효과 |
|---|---|---|
| `--only-tests` | `-OnlyTests` | pytest 만 (CI 와 동일) |
| `--skip-compare` | `-SkipCompare` | `compare_archs.py` 건너뜀 (수 분 절약) |
| `--skip-demo` | `-SkipDemo` | `notebook_demo.py` 건너뜀 |

## 빠른 사용 흐름

### 첫 배포 (PC → Jetson)
```bash
export FSD_HOST=jetson@10.0.0.42
export FSD_MODEL=./model.onnx
export FSD_CALIB=./stereo_calib.json
./scripts/deploy.sh           # rsync + 빌드 (재시작 X)
```

### Jetson 위에서 systemd 등록 (1회)
```bash
ssh jetson@10.0.0.42
cd ~/fsd
sudo ./scripts/install_systemd.sh
sudo systemctl enable fsd-jetson
sudo systemctl start fsd-jetson
```

### 모델만 업데이트 + 재시작
```bash
FSD_MODEL=./new_model.onnx ./scripts/deploy.sh --restart
```

### 로그 확인
```bash
ssh jetson@10.0.0.42 sudo journalctl -fu fsd-jetson
```

## 환경변수 (deploy.sh)

| 변수 | 기본 | 설명 |
|---|---|---|
| `FSD_HOST` | (필수) | SSH 대상 |
| `FSD_REMOTE_DIR` | `fsd` | 원격 작업 디렉터리 |
| `FSD_FEATURES` | `camera,gamepad,onnx-tensorrt,slam-opencv` | cargo features |
| `FSD_MODEL` | (없음) | `.onnx` 모델 파일 — 있으면 함께 SCP |
| `FSD_CALIB` | (없음) | `stereo_calib.json` — 있으면 함께 SCP |
