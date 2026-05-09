"""Stereo 깊이 처리 데모. 캘리브레이션 + 한 쌍의 카메라 이미지 → depth map + 장애물 ROI 시각화.

용도:
  - 캘리브레이션 결과(stereo_calib.json) sanity 검증
  - SLAM 통합 전에 disparity/depth 알고리즘 튜닝
  - jetson 의 Rust opencv_impl 과 동일한 알고리즘 (참조 구현)

사용:
  python stereo_demo.py --calib ../stereo_calib.json \
                         --left  ../recordings/run-001/cam0/00001000.jpg \
                         --right ../recordings/run-001/cam1/00001000.jpg \
                         --out depth_demo.png

알고리즘:
  1. 캘리브레이션에서 rectification map 계산 (1회, init 시점)
  2. 좌/우 영상을 rectify (epipolar line 평행화)
  3. StereoSGBM 으로 disparity 계산
  4. disparity → depth 변환 (z = fx · baseline / disparity)
  5. ROI 안에서 가까운(< 1 m) 픽셀 비율 = obstacle ratio
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import cv2
import matplotlib.pyplot as plt
import numpy as np


def build_rectifier(calib: dict, image_size: tuple[int, int]):
    """rectify 매핑 + Q 행렬(disparity → 3D 변환용)."""
    k_l = np.array(calib["left"]["k"], dtype=np.float64).reshape(3, 3)
    d_l = np.array(calib["left"]["dist"], dtype=np.float64)
    k_r = np.array(calib["right"]["k"], dtype=np.float64).reshape(3, 3)
    d_r = np.array(calib["right"]["dist"], dtype=np.float64)
    R = np.array(calib["extrinsics"]["r"], dtype=np.float64).reshape(3, 3)
    T = np.array(calib["extrinsics"]["t"], dtype=np.float64)

    R1, R2, P1, P2, Q, _, _ = cv2.stereoRectify(
        k_l, d_l, k_r, d_r, image_size, R, T,
        flags=cv2.CALIB_ZERO_DISPARITY, alpha=0,
    )
    map1_l, map2_l = cv2.initUndistortRectifyMap(k_l, d_l, R1, P1, image_size, cv2.CV_16SC2)
    map1_r, map2_r = cv2.initUndistortRectifyMap(k_r, d_r, R2, P2, image_size, cv2.CV_16SC2)
    return (map1_l, map2_l), (map1_r, map2_r), Q


def make_sgbm(num_disparities: int = 64, block_size: int = 7) -> cv2.StereoSGBM:
    return cv2.StereoSGBM_create(
        minDisparity=0,
        numDisparities=num_disparities,
        blockSize=block_size,
        P1=8 * 3 * block_size ** 2,
        P2=32 * 3 * block_size ** 2,
        disp12MaxDiff=1,
        uniquenessRatio=10,
        speckleWindowSize=100,
        speckleRange=2,
        preFilterCap=63,
        mode=cv2.STEREO_SGBM_MODE_SGBM_3WAY,
    )


def compute_depth(left_bgr: np.ndarray, right_bgr: np.ndarray, calib: dict
                  ) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    h, w = left_bgr.shape[:2]
    image_size = (w, h)
    (m1l, m2l), (m1r, m2r), Q = build_rectifier(calib, image_size)
    left_r = cv2.remap(left_bgr, m1l, m2l, cv2.INTER_LINEAR)
    right_r = cv2.remap(right_bgr, m1r, m2r, cv2.INTER_LINEAR)

    gray_l = cv2.cvtColor(left_r, cv2.COLOR_BGR2GRAY)
    gray_r = cv2.cvtColor(right_r, cv2.COLOR_BGR2GRAY)

    sgbm = make_sgbm()
    # SGBM 출력은 fixed-point (16x), float 로 변환.
    disp = sgbm.compute(gray_l, gray_r).astype(np.float32) / 16.0

    # 3D 재투영. xyz[h, w, :] = (X, Y, Z) m 단위.
    xyz = cv2.reprojectImageTo3D(disp, Q)
    depth = xyz[:, :, 2]
    # disparity 가 0 이하인 픽셀은 무효.
    depth[disp <= 0] = 0.0
    return left_r, depth, disp


def obstacle_ratio(depth: np.ndarray, roi: tuple[int, int, int, int],
                   max_distance_m: float) -> float:
    x, y, w, h = roi
    region = depth[y:y + h, x:x + w]
    valid = region > 0
    n_valid = int(valid.sum())
    if n_valid == 0:
        return 0.0
    close = (region < max_distance_m) & valid
    return float(close.sum()) / float(n_valid)


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--calib", type=Path, required=True)
    p.add_argument("--left", type=Path, required=True)
    p.add_argument("--right", type=Path, required=True)
    p.add_argument("--out", type=Path, default=Path("depth_demo.png"))
    p.add_argument("--max-distance", type=float, default=1.0,
                   help="장애물 임계 거리 (m)")
    args = p.parse_args()

    calib = json.loads(args.calib.read_text())
    left_bgr = cv2.imread(str(args.left))
    right_bgr = cv2.imread(str(args.right))
    if left_bgr is None or right_bgr is None:
        raise SystemExit("이미지 로드 실패")

    left_r, depth, disp = compute_depth(left_bgr, right_bgr, calib)

    h, w = depth.shape
    # 차량 전방 ROI: 화면 가운데 50% × 하단 50%.
    roi_x = w // 4
    roi_y = h // 2
    roi_w = w // 2
    roi_h = h // 2
    ratio = obstacle_ratio(depth, (roi_x, roi_y, roi_w, roi_h), args.max_distance)

    fig, axes = plt.subplots(1, 3, figsize=(16, 5))
    fig.suptitle(f"obstacle_ratio (close < {args.max_distance:.1f}m) = {ratio:.2%}")

    ax = axes[0]
    ax.imshow(cv2.cvtColor(left_r, cv2.COLOR_BGR2RGB))
    ax.add_patch(plt.Rectangle((roi_x, roi_y), roi_w, roi_h, fill=False,
                               edgecolor="red", linewidth=2))
    ax.set_title("left rectified + obstacle ROI")
    ax.axis("off")

    ax = axes[1]
    valid_mask = disp > 0
    if valid_mask.any():
        vmin, vmax = float(np.percentile(disp[valid_mask], 5)), float(np.percentile(disp[valid_mask], 95))
        im = ax.imshow(disp, cmap="plasma", vmin=vmin, vmax=vmax)
        plt.colorbar(im, ax=ax, fraction=0.046, label="disparity (px)")
    else:
        ax.imshow(disp, cmap="plasma")
    ax.set_title("disparity")
    ax.axis("off")

    ax = axes[2]
    depth_view = np.where(depth > 0, depth, np.nan)
    im = ax.imshow(depth_view, cmap="viridis_r", vmin=0.3, vmax=5.0)
    plt.colorbar(im, ax=ax, fraction=0.046, label="depth (m)")
    ax.set_title("depth (m)")
    ax.axis("off")

    plt.tight_layout()
    plt.savefig(args.out, dpi=120)
    print(f"wrote {args.out}")
    print(f"obstacle ratio = {ratio:.2%}")


if __name__ == "__main__":
    main()
