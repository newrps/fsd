"""듀얼 카메라 stereo 캘리브레이션 — 체커보드 패턴 사용.

사용:
  1. 체커보드 (예: 9x6, 25mm 셀) 출력 + 평평한 표면에 부착
  2. fsd-jetson record 로 cam0/cam1 양쪽에서 다양한 각도로 ~20 쌍 캡처
  3. python calibrate.py --left recordings/cal/cam0/ --right recordings/cal/cam1/ \
                          --pattern 9x6 --cell 0.025 --out stereo_calib.json

출력 stereo_calib.json 의 포맷은 jetson/src/slam.rs::StereoCalibration 와 1:1 일치.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import cv2
import numpy as np


def find_corners(images: list[Path], pattern: tuple[int, int]) -> list[np.ndarray]:
    corners_list = []
    used = []
    flags = cv2.CALIB_CB_ADAPTIVE_THRESH | cv2.CALIB_CB_NORMALIZE_IMAGE
    for path in images:
        img = cv2.imread(str(path), cv2.IMREAD_GRAYSCALE)
        if img is None:
            continue
        ok, corners = cv2.findChessboardCorners(img, pattern, flags=flags)
        if not ok:
            continue
        cv2.cornerSubPix(
            img, corners, (11, 11), (-1, -1),
            (cv2.TERM_CRITERIA_EPS + cv2.TERM_CRITERIA_MAX_ITER, 30, 0.001),
        )
        corners_list.append(corners)
        used.append(path)
    return corners_list, used


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--left", type=Path, required=True, help="cam0 폴더")
    p.add_argument("--right", type=Path, required=True, help="cam1 폴더")
    p.add_argument("--pattern", default="9x6", help="W x H inner corners")
    p.add_argument("--cell", type=float, default=0.025, help="셀 한 변 (m)")
    p.add_argument("--out", type=Path, default=Path("stereo_calib.json"))
    args = p.parse_args()

    pw, ph = (int(x) for x in args.pattern.split("x"))
    pattern_size = (pw, ph)

    left_imgs = sorted(args.left.glob("*.jpg"))
    right_imgs = sorted(args.right.glob("*.jpg"))
    if len(left_imgs) != len(right_imgs):
        raise SystemExit(f"좌/우 프레임 수가 다름: {len(left_imgs)} vs {len(right_imgs)}")

    print(f"이미지 {len(left_imgs)} 쌍에서 corner 검출...")
    left_corners, l_used = find_corners(left_imgs, pattern_size)
    right_corners, r_used = find_corners(right_imgs, pattern_size)

    # 양쪽 모두 검출된 frame 만 사용.
    paired = [(l, r) for l, lp in zip(left_corners, l_used)
              for r, rp in zip(right_corners, r_used) if lp.stem == rp.stem]
    if len(paired) < 8:
        raise SystemExit(f"쌍이 너무 적음 ({len(paired)}). 최소 8쌍 권장.")

    obj_points = []
    objp = np.zeros((pw * ph, 3), np.float32)
    objp[:, :2] = np.mgrid[0:pw, 0:ph].T.reshape(-1, 2) * args.cell
    img_left = []
    img_right = []
    for l, r in paired:
        obj_points.append(objp.copy())
        img_left.append(l)
        img_right.append(r)

    h, w = cv2.imread(str(left_imgs[0]), cv2.IMREAD_GRAYSCALE).shape
    image_size = (w, h)

    print("좌측 카메라 단독 캘리브레이션...")
    _, k_l, d_l, _, _ = cv2.calibrateCamera(obj_points, img_left, image_size, None, None)
    print("우측 카메라 단독 캘리브레이션...")
    _, k_r, d_r, _, _ = cv2.calibrateCamera(obj_points, img_right, image_size, None, None)

    print("stereo 캘리브레이션...")
    flags = cv2.CALIB_FIX_INTRINSIC
    _, _, _, _, _, R, T, _, _ = cv2.stereoCalibrate(
        obj_points, img_left, img_right, k_l, d_l, k_r, d_r, image_size, flags=flags,
    )

    calib = {
        "left": {
            "k": k_l.flatten().tolist(),
            "dist": d_l.flatten().tolist()[:5],
            "width": w,
            "height": h,
        },
        "right": {
            "k": k_r.flatten().tolist(),
            "dist": d_r.flatten().tolist()[:5],
            "width": w,
            "height": h,
        },
        "extrinsics": {
            "r": R.flatten().tolist(),
            "t": T.flatten().tolist(),
        },
    }
    args.out.write_text(json.dumps(calib, indent=2))
    baseline = float(np.linalg.norm(T))
    print(f"wrote {args.out}  baseline={baseline*1000:.1f} mm")


if __name__ == "__main__":
    main()
