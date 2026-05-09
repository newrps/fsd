# slam — 듀얼 카메라 Stereo 깊이/장애물 (스펙 3.2.2)

`jetson/src/slam.rs` + `ml-py/calibrate.py`. 현재는 **인터페이스 + stub** 단계.

## 단계별 로드맵

| 단계 | 설명 | 상태 |
|---|---|---|
| 1. 캘리브레이션 | 체커보드로 stereo intrinsic/extrinsic 측정 → `stereo_calib.json` | `ml-py/calibrate.py` 작성 완료 |
| 2. Rectification | 두 카메라 영상을 epipolar line 평행화 | TODO (opencv 바인딩) |
| 3. Disparity | StereoSGBM 또는 신경망 기반 disparity 계산 | TODO |
| 4. Depth 변환 | `z = fx · baseline / disparity` | TODO |
| 5. 장애물 감지 | ROI 안에서 가까운 픽셀 비율 → emergency reduce | `obstacle_ratio` 함수 완료 |
| 6. (장기) VO/Mapping | ORB-SLAM3 또는 자체 visual odometry | 미정 |

## 캘리브레이션 절차

1. **체커보드 출력**: 9×6 inner corners, 셀 25 mm. 평평한 골판지에 부착
2. **데이터 수집**:
   ```bash
   fsd-jetson record --out recordings/cal --fps 5 --input gamepad
   # 차를 손으로 들고 체커보드를 다양한 거리/각도/회전으로 ~30 쌍 캡처
   ```
3. **캘리브레이션**:
   ```bash
   cd ml-py
   python calibrate.py --left ../recordings/cal/cam0 --right ../recordings/cal/cam1 \
                       --pattern 9x6 --cell 0.025 --out ../stereo_calib.json
   ```
4. **결과 확인**: baseline 이 실제 카메라 간 거리(약 80 mm) 와 일치하는지 확인

## 데이터 구조

`StereoCalibration` (Rust ↔ JSON 1:1):

```json
{
  "left":  {"k": [fx,0,cx, 0,fy,cy, 0,0,1], "dist": [k1,k2,p1,p2,k3], "width": 1280, "height": 720},
  "right": {...},
  "extrinsics": {"r": [9 floats row-major], "t": [tx,ty,tz]}
}
```

## 구현 옵션 비교

| 백엔드 | 정확도 | 속도 (Orin Nano, 720p) | 의존성 |
|---|---|---|---|
| **OpenCV StereoSGBM** | 중간 | 50–100 ms/frame | JetPack 의 OpenCV (이미 설치됨) |
| 순수 Rust (`imageproc`) | 낮음 | 매우 느림 | 의존성 없음 |
| Deep stereo (PSMNet 등) | 매우 높음 | 100–200 ms (TRT 최적화) | 추가 모델 학습 필요 |
| Jetson DeepStream | 매우 빠름 | < 30 ms | NVIDIA SDK 필요 |

## 구현 상태

- ✅ Python 데모 (`ml-py/stereo_demo.py`): `cv2.stereoRectify` + `cv2.StereoSGBM` + `cv2.reprojectImageTo3D` 로 disparity → depth → obstacle_ratio. matplotlib 시각화. 알고리즘·파라미터 검증용
- ✅ Rust `OpenCvProcessor` (`jetson/src/slam.rs::opencv_impl`): Python 데모와 1:1 동일 알고리즘. `--features slam-opencv` 로 활성화. 시스템 OpenCV 필요
- ⏸️ Jetson 에서 실제 빌드·런 검증: 하드웨어 + JetPack 환경 필요

## 사용 예 (Python)

캘리브레이션 결과로 한 쌍의 stereo 영상을 처리:

```bash
python stereo_demo.py \
    --calib ../stereo_calib.json \
    --left ../recordings/run-001/cam0/00001000.jpg \
    --right ../recordings/run-001/cam1/00001000.jpg \
    --out depth_demo.png
```

→ `depth_demo.png` 에 (left+ROI, disparity, depth) 3 패널 저장 + obstacle_ratio 출력

## 사용 예 (Rust, Jetson)

```bash
# JetPack 환경 (libopencv-dev 사전 설치)
cargo build --release -p fsd-jetson --features "camera,onnx-tensorrt,slam-opencv"
```

`slam.rs::opencv_impl::OpenCvProcessor::new(calib)` 로 한 번 init 한 후 매 프레임 `process(jpeg_l, jpeg_r)` 호출.

## 자율주행 루프 통합 (구현 완료)

```text
camera ──▶ (cam0_jpeg, cam1_jpeg) ──┬──▶ inference (cam0) ──▶ raw_throttle, steering
                                     │                                │
                                     └──▶ stereo task (별도 thread) ──┘
                                            │                         │
                                            ▼                         ▼
                                     obstacle_ratio ──▶ ObstacleMonitor.modulate_throttle
                                                                       │
                                                                       ▼
                                                              STM32 ◀ DriveCommand
```

**ObstacleMonitor 정책** (jetson/src/slam.rs):

| obstacle_ratio | 동작 |
|---|---|
| `<= slow_ratio` (default 0.15) | 그대로 통과 |
| `slow_ratio..stop_ratio` | 선형 감속 (factor: 1.0 → 0.0) |
| `>= stop_ratio` (default 0.30) | throttle 0 (정지) |

**ROI**: 영상 가운데 50% × 하단 50% (차량 전방). 거리 임계: 1.0 m (default).

**런타임 활성화 조건**:
1. `--features camera,slam-opencv` 로 빌드
2. `fsd-jetson drive --calib stereo_calib.json --model model.onnx`
3. 캘리브레이션 로드 + OpenCvProcessor init 성공

조건 만족 안 되면: ObstacleMonitor 는 항상 ratio=0 으로 호출되어 **passthrough** (현재와 동일 동작).

## 추론 성능 영향

stereo 처리(SGBM)는 50–100 ms/frame 으로 무거움. `tokio::task::spawn_blocking` 으로 별도 OS thread 에서 실행해 50 Hz 추론 루프 차단 X. 매 추론 frame 의 cam1 을 stereo task 큐(capacity 2)에 try_send — 큐 가득 차면 drop. 즉 SLAM 은 가능한 만큼 처리하고, 메인 루프는 항상 가장 최근 obstacle_ratio 만 사용.
