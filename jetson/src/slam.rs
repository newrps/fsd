//! Visual SLAM 스캐폴드 — 듀얼 IMX219 카메라를 stereo pair 로 활용해 깊이/장애물 정보 산출.
//!
//! 단계 (스펙 3.2.2):
//!   1. 카메라 캘리브레이션 (intrinsic 각각 + extrinsic baseline)
//!   2. Stereo rectification
//!   3. Disparity → Depth
//!   4. 단순 장애물 감지 (depth threshold)
//!   5. (장기) Visual Odometry + 3D 맵
//!
//! 본 모듈은 **인터페이스 + stub 구현**. 실제 stereo 연산은:
//!   - 옵션 A: `opencv` crate (Jetson JetPack 의 OpenCV 바인딩) — feature `slam-opencv`
//!   - 옵션 B: 순수 Rust 구현 (`imageproc` 등) — 정확도/성능 한계
//!   - 옵션 C: GStreamer 의 nvdscamerafilter + DeepStream — Jetson 특화
//!
//! 캘리브레이션은 OpenCV 의 chessboard 패턴으로 PC 에서 미리 계산해 YAML/JSON 으로 저장.
//! `ml-py/calibrate.py` 참고.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 단일 카메라 intrinsic (pinhole + 5 param distortion).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraIntrinsics {
    /// 3x3 row-major: [fx, 0, cx, 0, fy, cy, 0, 0, 1]
    pub k: [f32; 9],
    /// [k1, k2, p1, p2, k3] (OpenCV 기본 distortion 모델)
    pub dist: [f32; 5],
    pub width: u32,
    pub height: u32,
}

/// 두 카메라 사이의 extrinsic. 좌측 카메라 좌표계 기준으로 우측 카메라의 위치/자세.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StereoExtrinsics {
    /// 3x3 row-major rotation matrix.
    pub r: [f32; 9],
    /// translation [tx, ty, tz] in meters. 가장 큰 성분이 baseline.
    pub t: [f32; 3],
}

/// 통합 stereo 캘리브레이션. JSON 파일로 저장/로드.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StereoCalibration {
    pub left: CameraIntrinsics,
    pub right: CameraIntrinsics,
    pub extrinsics: StereoExtrinsics,
}

impl StereoCalibration {
    pub fn load(path: &Path) -> Result<Self> {
        let s = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&s)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// baseline (m) — translation 벡터의 크기.
    pub fn baseline(&self) -> f32 {
        let t = &self.extrinsics.t;
        (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt()
    }
}

/// 깊이 맵 (m 단위). 0.0 = 미관측 (검은 픽셀, 텍스처 없음 등).
pub struct DepthMap {
    pub width: u32,
    pub height: u32,
    pub depth_m: Vec<f32>,
}

/// stereo 프레임 한 쌍을 받아 깊이를 계산하는 추상화.
pub trait StereoProcessor: Send {
    /// `left`/`right`: 같은 시점 캡처된 RGB 또는 grayscale JPEG bytes.
    fn process(&mut self, left: &[u8], right: &[u8]) -> Result<DepthMap>;
}

// ---------------------------------------------------------------------------
// Stub 구현 (slam feature 안 켜져 있을 때) — 항상 0 depth 반환.
// ---------------------------------------------------------------------------

pub struct StubProcessor {
    pub width: u32,
    pub height: u32,
}

impl StereoProcessor for StubProcessor {
    fn process(&mut self, _left: &[u8], _right: &[u8]) -> Result<DepthMap> {
        Ok(DepthMap {
            width: self.width,
            height: self.height,
            depth_m: vec![0.0; (self.width * self.height) as usize],
        })
    }
}

// ---------------------------------------------------------------------------
// OpenCV 기반 구현 — feature = "slam-opencv"
// ---------------------------------------------------------------------------
//
// 본 구현은 ml-py/stereo_demo.py 와 1:1 동일 알고리즘이며, 그 데모로 먼저 캘리브레이션·
// 파라미터를 검증한 후 동일 값을 그대로 사용한다.
//
// 의존성: opencv crate (시스템 OpenCV 4.5+ 필요).
// Jetson: JetPack 의 OpenCV 사용 (apt 로 libopencv-dev 추가 또는 JetPack 기본).
// PC: Windows 는 vcpkg / Linux 는 apt install libopencv-dev.
//
// 예상 성능 (Jetson Orin Nano, 1280x720, SGBM 64-disp): 50–100 ms/frame.
// 자율주행 50 Hz 루프 차단을 막으려면 별도 task 에서 N프레임마다 한 번씩 실행.

#[cfg(feature = "slam-opencv")]
pub mod opencv_impl {
    use super::*;
    use anyhow::{anyhow, Result};
    use opencv::{calib3d, core, imgcodecs, imgproc, prelude::*};

    pub struct OpenCvProcessor {
        calib: StereoCalibration,
        // rectification maps (CV_16SC2)
        map1_l: Mat,
        map2_l: Mat,
        map1_r: Mat,
        map2_r: Mat,
        // disparity → 3D 변환 행렬
        q_matrix: Mat,
        sgbm: core::Ptr<calib3d::StereoSGBM>,
        size: core::Size,
    }

    impl OpenCvProcessor {
        pub fn new(calib: StereoCalibration) -> Result<Self> {
            let size = core::Size::new(calib.left.width as i32, calib.left.height as i32);

            let k_l = mat_from_3x3(&calib.left.k)?;
            let d_l = mat_from_dist(&calib.left.dist)?;
            let k_r = mat_from_3x3(&calib.right.k)?;
            let d_r = mat_from_dist(&calib.right.dist)?;
            let r = mat_from_3x3(&calib.extrinsics.r)?;
            let t = mat_from_3x1(&calib.extrinsics.t)?;

            let mut r1 = Mat::default();
            let mut r2 = Mat::default();
            let mut p1 = Mat::default();
            let mut p2 = Mat::default();
            let mut q_matrix = Mat::default();
            let mut roi1 = core::Rect::default();
            let mut roi2 = core::Rect::default();

            calib3d::stereo_rectify(
                &k_l, &d_l, &k_r, &d_r, size, &r, &t,
                &mut r1, &mut r2, &mut p1, &mut p2, &mut q_matrix,
                calib3d::CALIB_ZERO_DISPARITY, 0.0, size, &mut roi1, &mut roi2,
            )?;

            let mut map1_l = Mat::default();
            let mut map2_l = Mat::default();
            let mut map1_r = Mat::default();
            let mut map2_r = Mat::default();
            calib3d::init_undistort_rectify_map(
                &k_l, &d_l, &r1, &p1, size, core::CV_16SC2, &mut map1_l, &mut map2_l,
            )?;
            calib3d::init_undistort_rectify_map(
                &k_r, &d_r, &r2, &p2, size, core::CV_16SC2, &mut map1_r, &mut map2_r,
            )?;

            // SGBM. ml-py/stereo_demo.py 와 같은 파라미터.
            let block_size = 7;
            let num_disp = 64;
            let sgbm = calib3d::StereoSGBM::create(
                0, num_disp, block_size,
                8 * 3 * block_size * block_size,
                32 * 3 * block_size * block_size,
                1, 63, 10, 100, 2,
                calib3d::StereoSGBM_MODE_SGBM_3WAY,
            )?;

            Ok(Self {
                calib, map1_l, map2_l, map1_r, map2_r, q_matrix, sgbm, size,
            })
        }
    }

    impl StereoProcessor for OpenCvProcessor {
        fn process(&mut self, left_jpeg: &[u8], right_jpeg: &[u8]) -> Result<DepthMap> {
            let l_buf = core::Vector::<u8>::from_slice(left_jpeg);
            let r_buf = core::Vector::<u8>::from_slice(right_jpeg);
            let left = imgcodecs::imdecode(&l_buf, imgcodecs::IMREAD_GRAYSCALE)?;
            let right = imgcodecs::imdecode(&r_buf, imgcodecs::IMREAD_GRAYSCALE)?;
            if left.empty() || right.empty() {
                return Err(anyhow!("imdecode failed"));
            }

            let mut left_r = Mat::default();
            let mut right_r = Mat::default();
            imgproc::remap(
                &left, &mut left_r, &self.map1_l, &self.map2_l,
                imgproc::INTER_LINEAR, core::BORDER_CONSTANT, core::Scalar::default(),
            )?;
            imgproc::remap(
                &right, &mut right_r, &self.map1_r, &self.map2_r,
                imgproc::INTER_LINEAR, core::BORDER_CONSTANT, core::Scalar::default(),
            )?;

            let mut disp_raw = Mat::default();
            self.sgbm.compute(&left_r, &right_r, &mut disp_raw)?;
            // SGBM 출력은 16x fixed-point (CV_16S). float 로 변환.
            let mut disp = Mat::default();
            disp_raw.convert_to(&mut disp, core::CV_32F, 1.0 / 16.0, 0.0)?;

            // 3D 재투영
            let mut xyz = Mat::default();
            calib3d::reproject_image_to_3d(&disp, &mut xyz, &self.q_matrix, true, core::CV_32F)?;

            // depth = xyz[:,:,2]. disp <= 0 인 픽셀은 0.
            let h = self.size.height as u32;
            let w = self.size.width as u32;
            let mut depth_m = vec![0.0f32; (w * h) as usize];
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let xyz_pt = *xyz.at_2d::<core::Vec3f>(y, x)?;
                    let d = *disp.at_2d::<f32>(y, x)?;
                    let z = if d > 0.0 { xyz_pt[2] } else { 0.0 };
                    depth_m[(y as u32 * w + x as u32) as usize] = z;
                }
            }
            Ok(DepthMap { width: w, height: h, depth_m })
        }
    }

    fn mat_from_3x3(arr: &[f32; 9]) -> Result<Mat> {
        let v: Vec<f64> = arr.iter().map(|&x| x as f64).collect();
        Ok(Mat::from_slice_2d(&[
            &v[0..3], &v[3..6], &v[6..9],
        ])?)
    }

    fn mat_from_3x1(arr: &[f32; 3]) -> Result<Mat> {
        let v: Vec<f64> = arr.iter().map(|&x| x as f64).collect();
        Ok(Mat::from_slice_2d(&[&v[0..1], &v[1..2], &v[2..3]])?)
    }

    fn mat_from_dist(arr: &[f32; 5]) -> Result<Mat> {
        let v: Vec<f64> = arr.iter().map(|&x| x as f64).collect();
        Ok(Mat::from_slice(&v)?.try_clone()?)
    }
}

// ---------------------------------------------------------------------------
// 단순 장애물 감지 (depth threshold)
// ---------------------------------------------------------------------------

/// 차량 전방의 일정 ROI 안에서 가까운 장애물(임계 거리 미만) 픽셀 비율을 반환.
/// 비율이 임계값 초과 시 호출자가 emergency stop 또는 speed reduction 결정.
pub fn obstacle_ratio(depth: &DepthMap, roi: Roi, max_distance_m: f32) -> f32 {
    let mut close = 0u32;
    let mut total = 0u32;
    for y in roi.y..(roi.y + roi.h).min(depth.height) {
        for x in roi.x..(roi.x + roi.w).min(depth.width) {
            let idx = (y * depth.width + x) as usize;
            let d = depth.depth_m[idx];
            if d > 0.0 {
                total += 1;
                if d < max_distance_m {
                    close += 1;
                }
            }
        }
    }
    if total == 0 {
        0.0
    } else {
        close as f32 / total as f32
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Roi {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 차량 전방 ROI 의 obstacle_ratio 를 받아 throttle 을 변조하는 정책.
#[derive(Debug, Clone, Copy)]
pub struct ObstacleMonitor {
    /// 비율 이 값 이상 → throttle 0.
    pub stop_ratio: f32,
    /// 비율 이 값 이상 → 선형 감속 시작.
    pub slow_ratio: f32,
    /// 이 거리 이내 픽셀을 "가까운 장애물" 로 카운트 (m).
    pub max_distance_m: f32,
    /// 깊이 이미지 상의 ROI (보통 차량 전방 중앙 하단).
    pub roi: Roi,
}

impl Default for ObstacleMonitor {
    fn default() -> Self {
        Self {
            stop_ratio: 0.30,
            slow_ratio: 0.15,
            max_distance_m: 1.0,
            // 디폴트 ROI 는 process 에서 영상 크기 받아 채워야 하므로 placeholder.
            roi: Roi { x: 0, y: 0, w: 0, h: 0 },
        }
    }
}

impl ObstacleMonitor {
    /// `obstacle_ratio` 0..1 을 받아 base throttle 을 변조해 반환.
    /// - `<= slow_ratio`             : 그대로 유지
    /// - `slow_ratio..stop_ratio`    : 선형 감속 (1.0 → 0.0)
    /// - `>= stop_ratio`             : 0
    pub fn modulate_throttle(&self, base_throttle: f32, obstacle_ratio: f32) -> f32 {
        if obstacle_ratio >= self.stop_ratio {
            return 0.0;
        }
        if obstacle_ratio <= self.slow_ratio {
            return base_throttle;
        }
        let span = (self.stop_ratio - self.slow_ratio).max(1e-6);
        let factor = 1.0 - (obstacle_ratio - self.slow_ratio) / span;
        base_throttle * factor.clamp(0.0, 1.0)
    }

    /// 영상 크기 기준 차량 전방 default ROI (가운데 50% × 하단 50%).
    pub fn default_roi(image_w: u32, image_h: u32) -> Roi {
        Roi {
            x: image_w / 4,
            y: image_h / 2,
            w: image_w / 2,
            h: image_h / 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_roundtrip() {
        let cal = StereoCalibration {
            left: CameraIntrinsics {
                k: [800.0, 0.0, 640.0, 0.0, 800.0, 360.0, 0.0, 0.0, 1.0],
                dist: [0.0; 5],
                width: 1280,
                height: 720,
            },
            right: CameraIntrinsics {
                k: [800.0, 0.0, 640.0, 0.0, 800.0, 360.0, 0.0, 0.0, 1.0],
                dist: [0.0; 5],
                width: 1280,
                height: 720,
            },
            extrinsics: StereoExtrinsics {
                r: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                t: [0.08, 0.0, 0.0],
            },
        };
        assert!((cal.baseline() - 0.08).abs() < 1e-6);
    }

    #[test]
    fn obstacle_monitor_pass_through_below_slow() {
        let m = ObstacleMonitor::default();
        assert!((m.modulate_throttle(0.5, 0.10) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn obstacle_monitor_stops_above_stop() {
        let m = ObstacleMonitor::default();
        assert_eq!(m.modulate_throttle(0.5, 0.40), 0.0);
        assert_eq!(m.modulate_throttle(0.5, 0.30), 0.0); // 경계 포함
    }

    #[test]
    fn obstacle_monitor_linear_slow() {
        let m = ObstacleMonitor::default();
        // slow=0.15, stop=0.30 → 0.225 가 한가운데 → factor 0.5 → throttle 0.5*0.5=0.25
        let out = m.modulate_throttle(0.5, 0.225);
        assert!((out - 0.25).abs() < 1e-6, "got {}", out);
    }

    #[test]
    fn obstacle_monitor_respects_negative_throttle() {
        // 음수 throttle (후진) 도 동일 정책 — 큰 비율이면 0.
        let m = ObstacleMonitor::default();
        assert_eq!(m.modulate_throttle(-0.5, 0.40), 0.0);
        // 후진 시 전방 장애물은 의미 약하지만 일관성 유지.
    }

    #[test]
    fn obstacle_detection_basic() {
        let depth = DepthMap {
            width: 4,
            height: 4,
            depth_m: vec![
                1.0, 1.0, 1.0, 1.0,
                0.5, 0.5, 1.0, 1.0,  // 가까운 영역
                0.5, 0.5, 1.0, 1.0,
                1.0, 1.0, 1.0, 1.0,
            ],
        };
        let roi = Roi { x: 0, y: 1, w: 2, h: 2 };
        let r = obstacle_ratio(&depth, roi, 0.7);
        assert!((r - 1.0).abs() < 1e-6, "all 4 cells in ROI < 0.7m");
    }
}
