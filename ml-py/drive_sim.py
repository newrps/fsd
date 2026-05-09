"""키보드 운전 시뮬레이터 — 단순 1인칭 도로 뷰 + bicycle 차량 모델 + manifest 녹화.

차량 카메라 없이도 운전 연습 + 데이터 수집을 할 수 있게 해 주는 도구.
출력 manifest 는 jetson 의 record 결과와 같은 형식 → 같은 train.py 로 학습 가능.

조작:
  ← / →     : 조향 (좌/우)
  ↑         : 전진 (스로틀 +)
  ↓         : 후진 (스로틀 -)
  space     : 스로틀 0 + 브레이크
  s         : 녹화 시작/일시정지
  q / ESC   : 종료

도로:
  사인파 중앙선이 화면 가운데에서 좌우로 흔들림. 차의 lateral offset + heading 이
  중앙선의 화면상 위치를 결정. 모델 입장에선 "선이 왼쪽이면 우회전, 오른쪽이면 좌회전".
  현실 도로의 매우 단순화된 abstraction 이지만 ML 파이프라인 검증·운전 연습에 충분.

사용:
  python drive_sim.py --out ../recordings/sim-001 --duration 120
"""

from __future__ import annotations

import argparse
import json
import math
import time
from datetime import datetime, timezone
from pathlib import Path

import cv2
import numpy as np


WIDTH, HEIGHT = 320, 180
ROAD_HORIZON = HEIGHT // 2
LANE_WIDTH_PX_NEAR = 180
LANE_WIDTH_PX_FAR = 30


class CarState:
    """간단한 bicycle model. 단위는 임의 (시뮬 스케일)."""

    MAX_SPEED = 6.0       # m/s 동등
    MAX_REVERSE = -3.0
    MAX_STEER_RAD = 0.45
    WHEEL_BASE = 0.3      # m
    ACCEL_PER_THROTTLE = 4.0  # m/s^2 per unit throttle
    BRAKE = 6.0
    DRAG = 0.3            # 자연 감속

    def __init__(self):
        self.x = 0.0       # 도로 중앙 기준 lateral offset (m)
        self.heading = 0.0 # 진행 방향 yaw (rad). 0 = 도로 평행
        self.speed = 0.0   # m/s

    def step(self, steering: float, throttle: float, dt: float) -> None:
        steering = max(-1.0, min(1.0, steering))
        throttle = max(-1.0, min(1.0, throttle))

        if throttle >= 0:
            self.speed += throttle * self.ACCEL_PER_THROTTLE * dt
        else:
            self.speed += throttle * self.ACCEL_PER_THROTTLE * dt
        # 자연 감속
        if self.speed > 0:
            self.speed = max(0.0, self.speed - self.DRAG * dt)
        elif self.speed < 0:
            self.speed = min(0.0, self.speed + self.DRAG * dt)

        self.speed = max(self.MAX_REVERSE, min(self.MAX_SPEED, self.speed))

        steer_rad = steering * self.MAX_STEER_RAD
        # heading 변화율
        if abs(self.speed) > 0.01:
            self.heading += (self.speed / self.WHEEL_BASE) * math.tan(steer_rad) * dt

        # 도로 중앙 기준 lateral 변화. forward 방향만 도로 따라간다고 가정.
        self.x += self.speed * math.sin(self.heading) * dt


def road_centerline_offset(t: float) -> float:
    """시점 t (초) 의 도로 중앙선 lateral offset (m). 사인파."""
    return 1.5 * math.sin(t * 0.3)


def render(state: CarState, t: float) -> np.ndarray:
    """1인칭 도로 뷰 한 장. (HEIGHT, WIDTH, 3) BGR."""
    img = np.zeros((HEIGHT, WIDTH, 3), dtype=np.uint8)
    # 하늘
    img[:ROAD_HORIZON] = (180, 130, 100)  # 어두운 청회색
    # 도로
    img[ROAD_HORIZON:] = (60, 60, 60)

    # 도로 중앙선 — 화면상의 위치는 차의 lateral offset + heading 에 영향받음.
    # 차 기준 도로 중앙은 (road_x - state.x). heading 으로 회전.
    road_x = road_centerline_offset(t)
    relative_x = road_x - state.x  # 양수면 도로 중앙이 차의 우측

    # 화면상의 픽셀 매핑: 1m = 60 px (가까운 곳 기준).
    # vanishing point 는 화면 가운데 ROAD_HORIZON, 그곳에서 relative_x 가 약간만 영향.
    vp_x = WIDTH / 2 + state.heading * 80  # heading 만큼 vanishing point 가 화면에서 이동
    base_x = WIDTH / 2 + relative_x * 60   # 가까운 곳에서의 도로 중앙

    # 중앙선을 horizon 부터 화면 하단까지 그린다 (사다리꼴).
    for i, y in enumerate(range(ROAD_HORIZON, HEIGHT)):
        progress = (y - ROAD_HORIZON) / (HEIGHT - ROAD_HORIZON)
        x = vp_x + (base_x - vp_x) * progress
        # 점선
        if (i // 12) % 2 == 0:
            xi = int(x)
            if 0 <= xi - 1 < WIDTH and xi + 2 < WIDTH:
                img[y, max(0, xi - 1):min(WIDTH, xi + 2)] = (255, 255, 255)

    # 좌/우 차선 가장자리.
    for side in (-1, +1):
        for y in range(ROAD_HORIZON, HEIGHT):
            progress = (y - ROAD_HORIZON) / (HEIGHT - ROAD_HORIZON)
            half_w = LANE_WIDTH_PX_FAR + (LANE_WIDTH_PX_NEAR - LANE_WIDTH_PX_FAR) * progress
            x = vp_x + (base_x - vp_x) * progress + side * half_w
            xi = int(x)
            if 0 <= xi < WIDTH:
                img[y, xi] = (200, 200, 80)
    return img


def hud(img: np.ndarray, state: CarState, steering: float, throttle: float,
        recording: bool, t: float, n_frames: int) -> np.ndarray:
    """우상단에 시뮬 상태 표시."""
    h, w = img.shape[:2]
    # HUD 배경
    bg = img.copy()
    cv2.rectangle(bg, (0, 0), (w, 22), (0, 0, 0), -1)
    img = cv2.addWeighted(bg, 0.6, img, 0.4, 0)
    cv2.putText(img, f"st={steering:+.2f} th={throttle:+.2f} v={state.speed:+.1f}",
                (4, 14), cv2.FONT_HERSHEY_SIMPLEX, 0.4, (255, 255, 255), 1)
    cv2.putText(img, f"REC {n_frames}" if recording else "PAUSE",
                (w - 70, 14), cv2.FONT_HERSHEY_SIMPLEX, 0.4,
                (0, 0, 255) if recording else (0, 255, 255), 1)
    return img


def auto_inputs(t: float) -> tuple[float, float]:
    """헤드리스 자동 운전 — 사인파 steering + 약한 throttle. 학습 가능한 패턴 보장."""
    # 0.4 Hz 짜리 사인파 + 약한 잡음 없는 결정론적 입력. 도로 사인파(0.3 Hz) 와 다른 주파수 →
    # 모델이 "도로 위치 → steering" 매핑을 진짜로 배워야 함.
    steering = 0.7 * math.sin(t * 2 * math.pi * 0.4)
    throttle = 0.4
    return steering, throttle


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, required=True, help="manifest 출력 디렉터리")
    p.add_argument("--duration", type=float, default=120.0, help="최대 녹화 시간(초)")
    p.add_argument("--fps", type=int, default=30)
    p.add_argument("--auto", action="store_true",
                   help="GUI 없이 사인파 입력으로 자동 운전 + 녹화 (CI/검증용)")
    args = p.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "cam0").mkdir(exist_ok=True)
    (args.out / "cam1").mkdir(exist_ok=True)
    manifest_f = (args.out / "manifest.jsonl").open("w", encoding="utf-8")

    state = CarState()
    steering = 0.0
    throttle = 0.0
    recording = False
    seq = 0
    t_sim = 0.0
    dt = 1.0 / args.fps
    t_start = time.perf_counter()

    if args.auto:
        print(f"AUTO mode - duration={args.duration:.0f}s fps={args.fps} out={args.out}")
        recording = True
    else:
        print("키보드: ←→ 조향 / ↑↓ 스로틀 / space 브레이크 / s 녹화토글 / q 종료")
        print(f"출력: {args.out}")

    try:
        while time.perf_counter() - t_start < args.duration:
            if args.auto:
                # 헤드리스: 시뮬 시간만 진행, GUI 안 띄움.
                steering, throttle = auto_inputs(t_sim)
                state.step(steering, throttle, dt)
                t_sim += dt
                img = render(state, t_sim)
                cam0_rel = f"cam0/{seq:08d}.jpg"
                cam1_rel = f"cam1/{seq:08d}.jpg"
                cv2.imwrite(str(args.out / cam0_rel), img,
                            [cv2.IMWRITE_JPEG_QUALITY, 85])
                shifted = np.zeros_like(img)
                shifted[:, 12:] = img[:, :-12]
                cv2.imwrite(str(args.out / cam1_rel), shifted,
                            [cv2.IMWRITE_JPEG_QUALITY, 85])
                manifest_f.write(json.dumps({
                    "seq": seq, "t": datetime.now(timezone.utc).isoformat(),
                    "steering": float(steering), "throttle": float(throttle),
                    "cam0": cam0_rel, "cam1": cam1_rel,
                }) + "\n")
                seq += 1
                if args.duration > 0 and t_sim >= args.duration:
                    break
                continue
            # 입력 (cv2 keypress).
            key = cv2.waitKey(int(dt * 1000)) & 0xFF
            if key == ord('q') or key == 27:  # ESC
                break
            elif key == ord('s'):
                recording = not recording
                print(f"recording = {recording}")
            elif key == 0x4B or key == 81:  # left arrow (Windows: 0x4B; Linux: 81)
                steering = max(-1.0, steering - 0.1)
            elif key == 0x4D or key == 83:  # right
                steering = min(1.0, steering + 0.1)
            elif key == 0x48 or key == 82:  # up
                throttle = min(1.0, throttle + 0.1)
            elif key == 0x50 or key == 84:  # down
                throttle = max(-1.0, throttle - 0.1)
            elif key == ord(' '):
                throttle = 0.0
                state.speed *= 0.5
            elif key == ord('a'):
                steering = -1.0
            elif key == ord('d'):
                steering = +1.0
            elif key == ord('w'):
                throttle = +1.0
            elif key == ord('x'):
                throttle = -1.0
            elif key == ord('c'):
                steering = 0.0
                throttle = 0.0
            # steering 자동 중립 복귀 (10%/frame)
            steering *= 0.92

            state.step(steering, throttle, dt)
            t_sim += dt

            img = render(state, t_sim)
            display = hud(img.copy(), state, steering, throttle, recording, t_sim, seq)
            display_big = cv2.resize(display, (WIDTH * 3, HEIGHT * 3), interpolation=cv2.INTER_NEAREST)
            cv2.imshow("fsd drive_sim — q to quit, s to toggle record", display_big)

            if recording:
                cam0_rel = f"cam0/{seq:08d}.jpg"
                cam1_rel = f"cam1/{seq:08d}.jpg"
                # cam1 = cam0 의 12 px 시프트 (매우 단순 stereo 시뮬)
                cv2.imwrite(str(args.out / cam0_rel), img,
                            [cv2.IMWRITE_JPEG_QUALITY, 85])
                shifted = np.zeros_like(img)
                shifted[:, 12:] = img[:, :-12]
                cv2.imwrite(str(args.out / cam1_rel), shifted,
                            [cv2.IMWRITE_JPEG_QUALITY, 85])
                manifest_f.write(json.dumps({
                    "seq": seq,
                    "t": datetime.now(timezone.utc).isoformat(),
                    "steering": float(steering),
                    "throttle": float(throttle),
                    "cam0": cam0_rel, "cam1": cam1_rel,
                }) + "\n")
                seq += 1
    finally:
        manifest_f.close()
        cv2.destroyAllWindows()
        print(f"녹화 종료. 총 {seq} 프레임 저장 → {args.out / 'manifest.jsonl'}")
        if seq > 0:
            print(f"이제 학습 가능:")
            print(f"  python train.py --manifest {args.out / 'manifest.jsonl'} --out ckpts --epochs 30")


if __name__ == "__main__":
    main()
