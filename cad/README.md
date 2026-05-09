# cad — 3D CAD (CadQuery)

CadQuery 기반의 파라메트릭 마운트 부품 스크립트.
파이썬 스크립트 실행 시 `build/<part>.stl` 과 `.step` 이 생성된다.

## 셋업

```bash
pip install cadquery
```

GUI 가 필요하면 [CQ-editor](https://github.com/CadQuery/CQ-editor) 또는 VSCode 의
[ocp-cad-viewer](https://github.com/bernhard-42/vscode-ocp-cad-viewer) 사용 권장.

## 부품 목록

| 파일 | 설명 |
|---|---|
| `camera_mount.py` | 듀얼 IMX219 마운트. 스테레오 베이스라인 파라미터(`BASELINE_MM`). |
| `jetson_mount.py` | Jetson Orin Nano Super 데브킷 마운트 플레이트. |
| `nucleo_mount.py` | NUCLEO-H753ZI 마운트 플레이트. |

각 스크립트 상단의 상수(대문자)들이 조정 가능 파라미터다.
**실제 차체(HSP 94118)와 보드의 정확한 hole pitch / 두께는 실측 후 보정**할 것.

## 빌드

```bash
mkdir -p build
python camera_mount.py
python jetson_mount.py
python nucleo_mount.py
ls build/
```

3D 프린팅 추천 설정: PLA/PETG, 0.2 mm layer, 30% infill, 2 perimeters. 진동 노출이 큰 부분은
infill 50% 이상 또는 ABS/ASA 권장.
