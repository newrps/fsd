//! Jetson Orin Nano Super 메인 애플리케이션.
//!
//! 네 가지 모드를 지원:
//!   - `serve`   : 시리얼 브리지만 (NEUTRAL 송신 + 텔레메트리 표시) — 개발/디버깅
//!   - `record`  : 듀얼 카메라 + 입력(RC/gamepad) 동기화 로깅 — 학습 데이터 수집
//!   - `drive`   : 학습된 모델로 자율 추론 + (선택) stereo obstacle 감지
//!   - `replay`  : 녹화된 manifest 를 모델에 통과시켜 오프라인 검증

mod serial;
mod logger;
#[cfg(feature = "camera")]
mod camera;
mod control;
mod inference;
mod input;
mod slam;

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "fsd-jetson",
    version,
    about = "fsd autonomous RC car — Jetson side controller",
    long_about = "4 modes: serve / record / drive / replay. \
                  See `<mode> --help` for details. RUST_LOG 으로 로그 레벨 제어 (info/debug/trace)."
)]
struct Cli {
    /// 시리얼 디바이스 경로. NUCLEO 가 ST-LINK VCP 로 잡히면 보통 /dev/ttyACM0.
    /// Jetson UART 핀 직결 시 /dev/ttyTHS1.
    #[arg(long, default_value = "/dev/ttyACM0")]
    serial: String,

    /// 시리얼 보드레이트. 펌웨어 USART 설정과 일치해야 함 (기본 921600).
    #[arg(long, default_value_t = 921_600)]
    baud: u32,

    #[command(subcommand)]
    cmd: Cmd,
}

/// `record --input` 으로 사용되는 입력 소스 선택.
#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "lower")]
enum InputKind {
    /// gamepad 가 init 되면 gamepad, 아니면 RC 로 fallback.
    Auto,
    /// RC 송신기 → STM32 펌웨어가 캡처해서 텔레메트리로 전달.
    Rc,
    /// USB 게임패드 (gilrs). `--features gamepad` 빌드 필요.
    Gamepad,
}

impl InputKind {
    fn as_str(self) -> &'static str {
        match self {
            InputKind::Auto => "auto",
            InputKind::Rc => "rc",
            InputKind::Gamepad => "gamepad",
        }
    }
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// 시리얼 브리지만 실행 (텔레메트리 stdout 표시).
    Serve,
    /// 데이터 수집 모드 — 카메라 + 입력 명령 동기화 로깅.
    Record {
        /// 출력 디렉터리. manifest.jsonl + cam0/*.jpg + cam1/*.jpg 가 생성됨.
        #[arg(long)]
        out: PathBuf,
        /// 카메라 캡처 fps.
        #[arg(long, default_value_t = 30)]
        fps: u32,
        /// 입력 소스. `--input rc|gamepad|auto`.
        #[arg(long, value_enum, default_value_t = InputKind::Auto)]
        input: InputKind,
    },
    /// 자율 주행 모드 (모델 추론 → 명령 송신).
    Drive {
        /// 학습된 모델(.mpk burn 또는 .onnx) 경로.
        #[arg(long)]
        model: PathBuf,
        /// stereo 캘리브레이션 JSON 경로. 지정 + `slam-opencv` feature 활성 시
        /// 듀얼 카메라로 obstacle 감지 → 자동 감속/정지.
        #[arg(long)]
        calib: Option<PathBuf>,
    },
    /// Replay 모드 — 녹화된 manifest 를 모델에 통과시켜 예측 vs 실측을 CSV 로 비교.
    /// 카메라/STM32 없이 모델 sanity 검증.
    Replay {
        /// recordings/<run> 디렉터리 (manifest.jsonl 포함).
        #[arg(long)]
        recording: PathBuf,
        /// 모델 파일 (.onnx 또는 .mpk).
        #[arg(long)]
        model: PathBuf,
        /// 출력 CSV 경로. 기본: <recording>/replay.csv
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_target(false)
        .init();

    let cli = Cli::parse();
    tracing::info!(serial = %cli.serial, baud = cli.baud, ?cli.cmd, "fsd-jetson starting");

    match cli.cmd {
        Cmd::Serve => control::run_serve(&cli.serial, cli.baud).await,
        Cmd::Record { out, fps, input } => control::run_record(&cli.serial, cli.baud, &out, fps, input.as_str()).await,
        Cmd::Drive { model, calib } => control::run_drive(&cli.serial, cli.baud, &model, calib.as_deref()).await,
        Cmd::Replay { recording, model, out } => control::run_replay(&recording, &model, out.as_deref()),
    }
}
