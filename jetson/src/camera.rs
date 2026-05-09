//! 듀얼 IMX219 카메라 캡처 — Jetson `nvarguscamerasrc` + GStreamer.
//!
//! Jetson Orin Nano Super 에서 IMX219 두 개를 CSI 포트에 연결한 경우, sensor-id 0/1 로 구분된다.
//! 본 모듈은 `gstreamer-rs` 로 두 파이프라인을 동시에 띄우고 appsink 에서 프레임을 가져와
//! tokio mpsc 로 전달한다.
//!
//! 주의: feature = "camera" 가 켜진 상태에서만 컴파일된다. (호스트 PC 빌드 시에는 stub 만 동작)

#![cfg(feature = "camera")]

use anyhow::{Context, Result};
use chrono::Utc;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::logger::LogItem;

pub struct CameraConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self { width: 1280, height: 720, fps: 30 }
    }
}

/// 단일 카메라 파이프라인 빌드. 출력은 NV12 → RGBA → JPEG.
fn build_pipeline(sensor_id: u32, cfg: &CameraConfig) -> Result<(gst::Pipeline, AppSink)> {
    let pipeline_str = format!(
        "nvarguscamerasrc sensor-id={sensor_id} ! \
         video/x-raw(memory:NVMM), width={w}, height={h}, framerate={fps}/1, format=NV12 ! \
         nvvidconv ! video/x-raw, format=RGBA ! \
         videoconvert ! jpegenc quality=85 ! \
         appsink name=sink emit-signals=false sync=false max-buffers=2 drop=true",
        sensor_id = sensor_id,
        w = cfg.width,
        h = cfg.height,
        fps = cfg.fps,
    );
    let pipeline = gst::parse::launch(&pipeline_str)
        .context("gst parse_launch")?
        .downcast::<gst::Pipeline>()
        .map_err(|_| anyhow::anyhow!("not a pipeline"))?;
    let sink = pipeline
        .by_name("sink")
        .context("appsink missing")?
        .downcast::<AppSink>()
        .map_err(|_| anyhow::anyhow!("appsink downcast"))?;
    Ok((pipeline, sink))
}

/// 듀얼 카메라 캡처 시작. 각 프레임 쌍마다 `last_cmd` 의 현재값과 묶어 `LogItem` 으로 보낸다.
pub async fn run(
    cfg: CameraConfig,
    last_cmd: Arc<tokio::sync::watch::Receiver<fsd_protocol::DriveCommand>>,
    out: mpsc::Sender<LogItem>,
) -> Result<()> {
    gst::init().context("gst init")?;
    let (p0, s0) = build_pipeline(0, &cfg)?;
    let (p1, s1) = build_pipeline(1, &cfg).ok().map(|(p, s)| (Some(p), Some(s))).unwrap_or((None, None));
    p0.set_state(gst::State::Playing)?;
    if let Some(p) = &p1 { p.set_state(gst::State::Playing)?; }

    let last_cmd = (*last_cmd).clone();

    tokio::task::spawn_blocking(move || -> Result<()> {
        loop {
            let s0_buf = match s0.pull_sample() {
                Ok(s) => s,
                Err(_) => break,
            };
            let cam0_jpeg = sample_to_vec(&s0_buf)?;
            let cam1_jpeg = if let Some(s1) = &s1 {
                match s1.try_pull_sample(gst::ClockTime::from_mseconds(5)) {
                    Some(s) => Some(sample_to_vec(&s)?),
                    None => None,
                }
            } else {
                None
            };
            let cmd = *last_cmd.borrow();
            let item = LogItem { t: Utc::now(), cmd, cam0_jpeg, cam1_jpeg };
            if out.blocking_send(item).is_err() {
                break;
            }
        }
        Ok(())
    })
    .await??;

    p0.set_state(gst::State::Null)?;
    if let Some(p) = p1 { p.set_state(gst::State::Null)?; }
    Ok(())
}

fn sample_to_vec(sample: &gst::Sample) -> Result<Vec<u8>> {
    let buffer = sample.buffer().context("sample buffer")?;
    let map = buffer.map_readable().map_err(|_| anyhow::anyhow!("map readable"))?;
    Ok(map.as_slice().to_vec())
}
