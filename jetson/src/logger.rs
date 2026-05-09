//! 데이터 로거 — 듀얼 카메라 프레임 + 시점 명령을 동기화하여 디스크에 기록.
//!
//! 디스크 레이아웃:
//! ```text
//!   <out>/manifest.jsonl              # 한 줄 = 하나의 샘플
//!   <out>/cam0/<seq>.jpg              # 좌측 카메라
//!   <out>/cam1/<seq>.jpg              # 우측 카메라
//! ```
//!
//! manifest.jsonl 의 각 줄은 `Sample` 직렬화 결과.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fsd_protocol::DriveCommand;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub seq: u64,
    pub t: DateTime<Utc>,
    pub steering: f32,
    pub throttle: f32,
    pub cam0: PathBuf,
    pub cam1: Option<PathBuf>,
}

/// JPEG 인코드된 한 쌍의 카메라 프레임 + 그 시점의 명령.
pub struct LogItem {
    pub t: DateTime<Utc>,
    pub cmd: DriveCommand,
    pub cam0_jpeg: Vec<u8>,
    pub cam1_jpeg: Option<Vec<u8>>,
}

pub struct Logger {
    out: PathBuf,
    rx: mpsc::Receiver<LogItem>,
    manifest: tokio::fs::File,
    seq: u64,
}

impl Logger {
    pub async fn create(out: &Path) -> Result<(mpsc::Sender<LogItem>, Self)> {
        fs::create_dir_all(out.join("cam0")).await?;
        fs::create_dir_all(out.join("cam1")).await?;
        let manifest = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(out.join("manifest.jsonl"))
            .await
            .context("open manifest.jsonl")?;
        let (tx, rx) = mpsc::channel(64);
        Ok((
            tx,
            Self {
                out: out.to_path_buf(),
                rx,
                manifest,
                seq: 0,
            },
        ))
    }

    pub async fn run(mut self) -> Result<()> {
        while let Some(item) = self.rx.recv().await {
            let seq = self.seq;
            self.seq += 1;
            let cam0_path = self.out.join(format!("cam0/{:08}.jpg", seq));
            fs::write(&cam0_path, &item.cam0_jpeg).await?;

            let cam1_path = if let Some(j) = &item.cam1_jpeg {
                let p = self.out.join(format!("cam1/{:08}.jpg", seq));
                fs::write(&p, j).await?;
                Some(p)
            } else {
                None
            };

            let sample = Sample {
                seq,
                t: item.t,
                steering: item.cmd.steering,
                throttle: item.cmd.throttle,
                cam0: cam0_path,
                cam1: cam1_path,
            };
            let mut line = serde_json::to_string(&sample)?;
            line.push('\n');
            self.manifest.write_all(line.as_bytes()).await?;
        }
        self.manifest.flush().await?;
        Ok(())
    }
}
