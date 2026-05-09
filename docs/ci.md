# CI — GitHub Actions

`.github/workflows/ci.yml`. push/PR 마다 자동 실행.

## 3 개 잡

| 잡 | 러너 | 무엇을 하나 |
|---|---|---|
| `rust-workspace` | ubuntu-latest | workspace test + check (default + 각 feature 조합) + fmt + clippy |
| `firmware` | ubuntu-latest | thumbv7em-none-eabihf 타겟으로 firmware check + release build |
| `python-smoke` | ubuntu-latest | Python venv → requirements 설치 → `smoke.py` (synthetic→train→ONNX→replay) |

총 실행 시간 ~5–10 분 (캐시 hit 시), 첫 실행 ~15 분.

## 캐싱

- Rust: `Swatinem/rust-cache@v2` 가 cargo registry + workspace target 자동 캐시
- Python: `actions/setup-python@v5` 의 pip 캐시

## 실패 시 디버깅

- Rust 빌드 실패: 워크플로 로그의 cargo 출력 확인. `error[E...]` 검색
- Smoke 실패: MAE 임계 초과면 모델 학습 회귀. ONNX export 실패면 PyTorch/onnxscript 버전 확인
- Firmware 실패: embassy 버전 변경됐을 가능성. troubleshooting.md 의 0.6 마이그레이션 표 참고

## 로컬 실행 (CI 와 동일 명령)

```bash
# Rust
cargo test --workspace
cargo check --workspace
cargo check -p fsd-jetson --features "gamepad,onnx,burn-inference"

# Firmware
cd firmware
cargo check
cargo build --release

# Python smoke
cd ml-py
pip install -r requirements.txt
python smoke.py
```

## 추가 가능한 잡 (TODO)

- macOS / Windows runner — Windows 는 ort download-binaries 이슈로 onnx feature skip 필요
- Jetson aarch64 cross-compile — qemu 또는 crossbuild
- `cargo audit` — 의존성 보안 취약점 스캔
- 코드 커버리지 (`cargo llvm-cov`)
