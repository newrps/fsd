//! 모드별 메인 루프.

use anyhow::Result;
use fsd_protocol::{DriveCommand, Frame};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::{interval, Duration};

use crate::input::{DriveInput, InputSource, RcSource};
use crate::serial;

/// 단순 브리지 — 텔레메트리만 출력.
pub async fn run_serve(path: &str, baud: u32) -> Result<()> {
    let mut framed = serial::open(path, baud).await?;
    let mut hb = interval(Duration::from_millis(100));
    let mut seq = 0u32;
    loop {
        tokio::select! {
            _ = hb.tick() => {
                let cmd = DriveCommand { seq, steering: 0.0, throttle: 0.0, estop: false };
                if let Err(e) = serial::send(&mut framed, Frame::Cmd(cmd)).await {
                    tracing::error!(?e, "send err");
                    break;
                }
                seq = seq.wrapping_add(1);
            }
            msg = serial::recv(&mut framed) => match msg {
                Some(Ok(Frame::Tlm(t))) => tracing::info!(?t, "tlm"),
                Some(Ok(other)) => tracing::debug!(?other, "rx"),
                Some(Err(e)) => tracing::warn!(?e, "rx err"),
                None => break,
            }
        }
    }
    Ok(())
}

/// 데이터 수집 모드.
///
/// - 시리얼 RX 루프: 텔레메트리에서 RC 값을 추출해 `rc_input_tx` watch 갱신.
/// - 입력 소스: --input=rc|gamepad|auto. auto 면 RC present 면 RC, 아니면 gamepad.
/// - 50 Hz 컨트롤 루프: 입력 → DriveCommand → STM32 송신 + cmd_tx watch 갱신.
/// - camera 태스크: cmd_tx 와 frame 을 묶어 logger 로 보냄.
pub async fn run_record(
    path: &str, baud: u32, out: &Path, fps: u32, input_kind: &str,
) -> Result<()> {
    let mut framed = serial::open(path, baud).await?;

    // 텔레메트리에서 RC 값을 watch 채널로 흘려보내기.
    let (rc_tx, rc_rx) = watch::channel(DriveInput::NEUTRAL);

    // logger 와 camera 가 공유할 "현재 명령" 채널.
    let (cmd_tx, cmd_rx) = watch::channel(DriveCommand::NEUTRAL);
    let cmd_rx_arc = Arc::new(cmd_rx);

    // 입력 소스 결정.
    let mut input_src = build_input_source(input_kind, rc_rx.clone())?;
    tracing::info!(name = input_src.name(), "input source");

    // logger 시작.
    let (log_tx, logger) = crate::logger::Logger::create(out).await?;
    let logger_handle = tokio::spawn(logger.run());

    // 카메라 시작 (feature 켜진 경우).
    #[cfg(feature = "camera")]
    let cam_handle = {
        let log_tx = log_tx.clone();
        let cmd_rx = cmd_rx_arc.clone();
        let cfg = crate::camera::CameraConfig { width: 1280, height: 720, fps };
        Some(tokio::spawn(async move {
            crate::camera::run(cfg, cmd_rx, log_tx).await
        }))
    };
    #[cfg(not(feature = "camera"))]
    {
        tracing::warn!("camera feature 가 꺼져있어 영상은 저장되지 않습니다. \
                        --features camera 로 빌드하세요.");
        let _ = (fps, cmd_rx_arc.clone());
    }

    // 50 Hz 입력 → 명령 송신 루프.
    let mut tick = interval(Duration::from_millis(20));
    let mut seq = 0u32;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let inp = input_src.poll();
                let cmd = DriveCommand {
                    seq,
                    steering: inp.steering,
                    throttle: inp.throttle,
                    estop: inp.estop || !inp.present,
                };
                seq = seq.wrapping_add(1);
                if let Err(e) = serial::send(&mut framed, Frame::Cmd(cmd)).await {
                    tracing::error!(?e, "send err");
                    break;
                }
                // logger 와 camera 가 같은 명령을 보도록 갱신.
                let _ = cmd_tx.send(cmd);
            }
            msg = serial::recv(&mut framed) => {
                match msg {
                    Some(Ok(Frame::Tlm(t))) => {
                        // RC 텔레메트리 값을 InputSource 가 볼 수 있게 watch 갱신.
                        let rc = DriveInput {
                            steering: if t.rc_steering.is_finite() { t.rc_steering } else { 0.0 },
                            throttle: if t.rc_throttle.is_finite() { t.rc_throttle } else { 0.0 },
                            estop: false,
                            present: t.rc_present,
                        };
                        let _ = rc_tx.send(rc);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => tracing::warn!(?e, "rx err"),
                    None => break,
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("ctrl-c, stopping");
                let _ = serial::send(&mut framed, Frame::Cmd(DriveCommand { seq, steering: 0.0, throttle: 0.0, estop: true })).await;
                break;
            }
        }
    }

    drop(log_tx);
    let _ = logger_handle.await;
    #[cfg(feature = "camera")]
    if let Some(h) = cam_handle { let _ = h.await; }
    Ok(())
}

fn build_input_source(
    kind: &str,
    rc_rx: tokio::sync::watch::Receiver<DriveInput>,
) -> Result<Box<dyn InputSource>> {
    match kind {
        "rc" => Ok(Box::new(RcSource::new(rc_rx))),
        "gamepad" => {
            #[cfg(feature = "gamepad")]
            {
                Ok(Box::new(crate::input::gamepad::GamepadSource::new()?))
            }
            #[cfg(not(feature = "gamepad"))]
            {
                let _ = rc_rx;
                anyhow::bail!("gamepad feature 가 꺼져 있습니다. --features gamepad 로 빌드하세요.")
            }
        }
        "auto" => {
            // gamepad 가능하면 둘 중 살아있는 쪽을 우선. 단순화: gamepad 있으면 gamepad, 없으면 rc.
            #[cfg(feature = "gamepad")]
            {
                match crate::input::gamepad::GamepadSource::new() {
                    Ok(g) => Ok(Box::new(g)),
                    Err(e) => {
                        tracing::warn!(?e, "gamepad init 실패, RC 로 fallback");
                        Ok(Box::new(RcSource::new(rc_rx)))
                    }
                }
            }
            #[cfg(not(feature = "gamepad"))]
            {
                Ok(Box::new(RcSource::new(rc_rx)))
            }
        }
        other => anyhow::bail!("unknown input kind: {other} (expected rc|gamepad|auto)"),
    }
}

/// Replay — 녹화된 manifest 를 모델에 통과시켜 예측 vs 실측 비교.
/// 카메라/시리얼 의존성 없음. 동기 함수 (tokio runtime 필요 X).
pub fn run_replay(recording: &Path, model: &Path, out: Option<&Path>) -> Result<()> {
    use serde::{Deserialize, Serialize};
    use std::fs::File;
    use std::io::{BufRead, BufReader, Write as _};
    use std::path::PathBuf;

    #[derive(Debug, Deserialize)]
    struct Sample {
        seq: u64,
        steering: f32,
        throttle: f32,
        cam0: PathBuf,
    }

    #[derive(Debug, Serialize)]
    struct Row {
        seq: u64,
        actual_steering: f32,
        actual_throttle: f32,
        pred_steering: f32,
        pred_throttle: f32,
        latency_us: u64,
    }

    let manifest = recording.join("manifest.jsonl");
    let f = File::open(&manifest)
        .map_err(|e| anyhow::anyhow!("open {:?}: {}", manifest, e))?;
    let out_path = out
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| recording.join("replay.csv"));

    #[allow(unused_mut)]
    let mut predictor = crate::inference::load(model)?;
    tracing::info!(?model, ?out_path, "replay starting");

    let mut writer = csv_open(&out_path)?;
    writeln!(
        writer,
        "seq,actual_steering,actual_throttle,pred_steering,pred_throttle,latency_us"
    )?;

    let mut total = 0usize;
    let mut bad = 0usize;
    let mut total_latency_us = 0u64;
    let mut sum_abs_err_steering = 0.0f64;
    let mut sum_abs_err_throttle = 0.0f64;

    for line in BufReader::new(f).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let s: Sample = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(?e, "skip bad manifest line");
                bad += 1;
                continue;
            }
        };
        let path = if s.cam0.is_absolute() {
            s.cam0.clone()
        } else {
            recording.join(&s.cam0)
        };
        let bytes = match std::fs::read(&path) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(?e, ?path, "skip missing image");
                bad += 1;
                continue;
            }
        };
        let chw = match jpeg_to_chw_replay(&bytes) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(?e, ?path, "decode");
                bad += 1;
                continue;
            }
        };
        let t0 = std::time::Instant::now();
        let (pred_s, pred_t) = predictor.predict(&chw)?;
        let lat_us = t0.elapsed().as_micros() as u64;
        total_latency_us += lat_us;
        sum_abs_err_steering += (pred_s - s.steering).abs() as f64;
        sum_abs_err_throttle += (pred_t - s.throttle).abs() as f64;
        total += 1;

        let row = Row {
            seq: s.seq,
            actual_steering: s.steering,
            actual_throttle: s.throttle,
            pred_steering: pred_s,
            pred_throttle: pred_t,
            latency_us: lat_us,
        };
        writeln!(
            writer,
            "{},{},{},{},{},{}",
            row.seq, row.actual_steering, row.actual_throttle,
            row.pred_steering, row.pred_throttle, row.latency_us,
        )?;
    }

    let avg_lat = if total > 0 { total_latency_us / total as u64 } else { 0 };
    let mae_s = if total > 0 { sum_abs_err_steering / total as f64 } else { 0.0 };
    let mae_t = if total > 0 { sum_abs_err_throttle / total as f64 } else { 0.0 };
    tracing::info!(
        total, bad, avg_latency_us = avg_lat,
        mae_steering = mae_s, mae_throttle = mae_t,
        ?out_path,
        "replay done"
    );
    println!(
        "samples processed = {} (bad skipped = {})",
        total, bad
    );
    println!("avg latency       = {} µs", avg_lat);
    println!("MAE steering      = {:.4}", mae_s);
    println!("MAE throttle      = {:.4}", mae_t);
    println!("CSV               = {}", out_path.display());
    Ok(())
}

fn csv_open(path: &Path) -> Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    Ok(std::fs::File::create(path)?)
}

fn jpeg_to_chw_replay(jpeg: &[u8]) -> Result<Vec<f32>> {
    use image::imageops::FilterType;
    let img = image::load_from_memory(jpeg)?.to_rgb8();
    let img = image::imageops::resize(&img, 200, 66, FilterType::Triangle);
    let mut chw = vec![0.0f32; 3 * 66 * 200];
    for (y, x, p) in img.enumerate_pixels() {
        for c in 0..3 {
            let idx = c * 66 * 200 + (y as usize) * 200 + (x as usize);
            chw[idx] = p.0[c] as f32 / 255.0;
        }
    }
    Ok(chw)
}

/// 자율주행 모드 — 카메라 입력 → 모델 추론 → 명령 송신.
/// 모델 확장자(.onnx / .mpk) 로 백엔드 자동 선택. (inference::load 참고)
/// stereo 캘리브레이션이 주어지고 `slam-opencv` feature 활성 시 obstacle 자동 감속/정지.
pub async fn run_drive(path: &str, baud: u32, model: &Path, calib: Option<&Path>) -> Result<()> {
    let mut framed = serial::open(path, baud).await?;
    #[allow(unused_mut)]
    let mut predictor = crate::inference::load(model)?;
    let _ = calib; // slam-opencv 안 켜진 빌드에서도 unused warn 안 나게.

    #[cfg(not(feature = "camera"))]
    {
        tracing::warn!(
            "camera feature 가 꺼져 있어 추론 입력이 없습니다. \
             --features camera 로 빌드하거나 별도 입력 소스를 wire 하세요. NEUTRAL 만 송신합니다."
        );
        let _ = predictor;
        let mut hb = interval(Duration::from_millis(20));
        let mut seq = 0u32;
        loop {
            hb.tick().await;
            let _ = serial::send(&mut framed, Frame::Cmd(DriveCommand { seq, steering: 0.0, throttle: 0.0, estop: false })).await;
            seq = seq.wrapping_add(1);
            tokio::select! {
                msg = serial::recv(&mut framed) => match msg {
                    Some(Ok(_)) => {}
                    Some(Err(e)) => tracing::warn!(?e, "rx err"),
                    None => break,
                },
                _ = tokio::signal::ctrl_c() => break,
            }
        }
        return Ok(());
    }

    #[cfg(feature = "camera")]
    {
        use crate::slam::ObstacleMonitor;

        let (cmd_tx, _cmd_rx) = tokio::sync::watch::channel(DriveCommand::NEUTRAL);
        let cmd_rx = std::sync::Arc::new(_cmd_rx);

        let (item_tx, mut item_rx) = tokio::sync::mpsc::channel::<crate::logger::LogItem>(8);
        let cam_handle = {
            let cmd_rx = cmd_rx.clone();
            tokio::spawn(async move {
                let cfg = crate::camera::CameraConfig { width: 1280, height: 720, fps: 30 };
                crate::camera::run(cfg, cmd_rx, item_tx).await
            })
        };
        let _ = cmd_tx;

        // ----- obstacle 감지 (stereo) -----
        // monitor 자체는 항상 활성. obstacle_ratio watch 의 default = 0.0 → throttle 패스.
        // slam-opencv feature 켜져 + calib 제공 시에만 실제 ratio 가 계산되어 watch 갱신.
        let monitor = ObstacleMonitor::default();
        let (obs_tx, obs_rx) = tokio::sync::watch::channel(0.0_f32);
        let _stereo_tx = setup_stereo_task(calib, obs_tx).await;

        let mut seq = 0u32;
        loop {
            tokio::select! {
                Some(item) = item_rx.recv() => {
                    let chw = match jpeg_to_chw(&item.cam0_jpeg) {
                        Ok(v) => v,
                        Err(e) => { tracing::warn!(?e, "decode"); continue; }
                    };
                    let (steering, throttle_raw) = match predictor.predict(&chw) {
                        Ok(v) => v,
                        Err(e) => { tracing::error!(?e, "predict"); (0.0, 0.0) }
                    };

                    // stereo task 가 있으면 cam1 도 보냄 (블로킹 X, 가득 차면 drop).
                    #[cfg(feature = "slam-opencv")]
                    if let (Some(tx), Some(c1)) = (_stereo_tx.as_ref(), item.cam1_jpeg.as_ref()) {
                        let _ = tx.try_send((item.cam0_jpeg.clone(), c1.clone()));
                    }

                    // obstacle 비율로 throttle 자동 변조.
                    let obstacle_ratio = *obs_rx.borrow();
                    let throttle = monitor.modulate_throttle(throttle_raw, obstacle_ratio);
                    if obstacle_ratio > monitor.slow_ratio {
                        tracing::info!(obstacle_ratio, throttle_raw, throttle, "obstacle: throttle modulated");
                    }

                    let cmd = DriveCommand { seq, steering, throttle, estop: false };
                    if let Err(e) = serial::send(&mut framed, Frame::Cmd(cmd)).await {
                        tracing::error!(?e, "send err"); break;
                    }
                    seq = seq.wrapping_add(1);
                }
                msg = serial::recv(&mut framed) => match msg {
                    Some(Ok(_)) => {}
                    Some(Err(e)) => tracing::warn!(?e, "rx err"),
                    None => break,
                },
                _ = tokio::signal::ctrl_c() => break,
            }
        }
        let _ = cam_handle.await;
        Ok(())
    }
}

/// stereo 처리 태스크 셋업. slam-opencv 가 꺼져 있으면 항상 None.
#[cfg(feature = "camera")]
async fn setup_stereo_task(
    _calib: Option<&Path>,
    _obs_tx: tokio::sync::watch::Sender<f32>,
) -> Option<tokio::sync::mpsc::Sender<(Vec<u8>, Vec<u8>)>> {
    #[cfg(feature = "slam-opencv")]
    {
        use crate::slam::{obstacle_ratio, opencv_impl::OpenCvProcessor, ObstacleMonitor, StereoCalibration, StereoProcessor};
        let calib_path = _calib?;
        let calibration = match StereoCalibration::load(calib_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(?e, ?calib_path, "stereo calib load 실패; obstacle 감지 비활성");
                return None;
            }
        };
        let mut proc = match OpenCvProcessor::new(calibration.clone()) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(?e, "OpenCvProcessor init 실패");
                return None;
            }
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(Vec<u8>, Vec<u8>)>(2);
        let img_w = calibration.left.width;
        let img_h = calibration.left.height;
        let roi = ObstacleMonitor::default_roi(img_w, img_h);
        let monitor = ObstacleMonitor { roi, ..Default::default() };

        tokio::task::spawn_blocking(move || {
            // SGBM 은 무거우므로 별도 thread.
            while let Some((l, r)) = rx.blocking_recv() {
                match proc.process(&l, &r) {
                    Ok(depth) => {
                        let ratio = obstacle_ratio(&depth, monitor.roi, monitor.max_distance_m);
                        let _ = _obs_tx.send(ratio);
                    }
                    Err(e) => tracing::warn!(?e, "stereo process"),
                }
            }
        });
        return Some(tx);
    }
    #[cfg(not(feature = "slam-opencv"))]
    {
        if _calib.is_some() {
            tracing::warn!(
                "--calib 가 주어졌지만 slam-opencv feature 가 꺼져있습니다. obstacle 감지 비활성."
            );
        }
        None
    }
}

#[cfg(feature = "camera")]
fn jpeg_to_chw(jpeg: &[u8]) -> anyhow::Result<Vec<f32>> {
    use image::imageops::FilterType;
    let img = image::load_from_memory(jpeg)?.to_rgb8();
    let img = image::imageops::resize(&img, 200, 66, FilterType::Triangle);
    let mut chw = vec![0.0f32; 3 * 66 * 200];
    for (y, x, p) in img.enumerate_pixels() {
        for c in 0..3 {
            let idx = c * 66 * 200 + (y as usize) * 200 + (x as usize);
            chw[idx] = p.0[c] as f32 / 255.0;
        }
    }
    Ok(chw)
}
