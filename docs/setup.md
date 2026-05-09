# setup — 개발 환경 구축

세 가지 환경을 구분해서 설명: **개발용 PC**, **Jetson Orin Nano Super**, **STM32 플래시 가능한 기기**(보통 PC).

## 공통 — Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup install stable
rustup component add rustfmt clippy
```

## 개발 PC (코드 작성, 학습, 펌웨어 빌드)

### Rust + 임베디드 타겟
```bash
rustup target add thumbv7em-none-eabihf            # STM32H7 용
cargo install probe-rs --features cli              # 펌웨어 플래시 + RTT 로그
cargo install cargo-binutils
rustup component add llvm-tools-preview
```

### Python (학습 — 경로 A 사용 시)
```bash
cd ml-py
python -m venv .venv
. .venv/bin/activate                               # Windows: .venv\Scripts\activate
pip install -r requirements.txt
```

### CAD (3D 모델 — 선택)
```bash
pip install cadquery
```

## Jetson Orin Nano Super

### JetPack
JetPack 6.x 권장. 설치 시 자동 포함:
- CUDA 12.x
- cuDNN
- TensorRT 10.x
- ONNX Runtime (Jetson 전용 빌드)

### Rust on Jetson
Jetson 은 aarch64. `rustup` 동일하게 설치 가능.

### GStreamer (camera feature)
JetPack 에 기본 포함되지만 dev 패키지 필요:
```bash
sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
```

### libudev (gamepad feature)
gilrs 가 evdev 를 통해 게임패드를 읽기 때문에 필요:
```bash
sudo apt install libudev-dev
sudo usermod -aG input $USER     # 권한, 재로그인 필요
```

### ONNX Runtime (onnx feature)
JetPack 의 ORT 를 사용하려면 `ort` 가 dynamic link 하도록 설정:
```bash
# JetPack 의 ORT 위치는 보통 /usr/lib/aarch64-linux-gnu/libonnxruntime.so
export ORT_DYLIB_PATH=/usr/lib/aarch64-linux-gnu/libonnxruntime.so
cargo build -p fsd-jetson --features "camera,onnx-tensorrt"
```

### Python on Jetson (선택, 학습을 Jetson 에서 할 때)
**`pip install torch` 금지.** NVIDIA 가 제공하는 Jetson 전용 PyTorch wheel 사용:
- https://forums.developer.nvidia.com 또는 `developer.download.nvidia.com/compute/redist/jp/`
- 예: `torch-2.x.x-cp310-cp310-linux_aarch64.whl`

## STM32 플래시 환경

PC 에 NUCLEO-H753ZI USB 연결. ST-LINK 가 USB CDC + 디버거로 잡힘.
- Linux: udev rule 추가 (probe-rs 문서 참고)
- Windows: ST-LINK 드라이버는 보통 자동, 필요 시 ST 사이트에서 설치
- macOS: 별도 드라이버 불필요

## 검증

```bash
cargo --version          # 1.7x+
rustup target list --installed | grep thumbv7em
probe-rs --version
python --version          # 3.10+
nvidia-smi                # Jetson 에서, 또는 GPU PC 에서
```
