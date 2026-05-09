//! 경로 C: burn 체크포인트 → ONNX export.
//!
//! ⚠️  현재 상태:
//!     burn 0.14 의 ONNX export 는 PR/Crate 가 아직 안정화 중입니다 (`burn-export-onnx`
//!     실험 단계). 본 바이너리는 인터페이스를 미리 잡아둔 **스텁**이며,
//!     PilotNet 같은 단순 CNN 도 burn 버전에 따라 export 가 실패할 수 있습니다.
//!
//! 권장 사용 우선순위:
//!     1) 가장 안전: ml-py/ Python 경로 — PyTorch 학습 + torch.onnx.export
//!     2) 차선   : 본 스텁을 burn 버전에 맞게 채워서 시도. 실패 시 1) 로 fallback.
//!     3) ONNX 안 거치고 burn 으로 직접 추론 (jetson --features burn-inference)
//!
//! 이 바이너리는 실패해도 다른 경로로 우회 가능하다는 점을 알립니다.

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "fsd-export-onnx")]
struct Args {
    /// burn 체크포인트(.mpk).
    #[arg(long)]
    ckpt: PathBuf,

    /// 출력 ONNX 파일.
    #[arg(long, default_value = "model.onnx")]
    out: PathBuf,

    /// opset 버전.
    #[arg(long, default_value_t = 17)]
    opset: u16,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    eprintln!("⚠️  burn → ONNX export (경로 C) 는 burn 버전마다 API 가 다릅니다.");
    eprintln!();
    eprintln!("    체크포인트 : {:?}", args.ckpt);
    eprintln!("    출력       : {:?}  (opset={})", args.out, args.opset);
    eprintln!();
    eprintln!("    burn 의 onnx export 가 안정되면 이 함수에서 다음을 호출:");
    eprintln!("        let model = PilotNetConfig::default().init::<B>(&device);");
    eprintln!("        let record = CompactRecorder::new().load(args.ckpt, &device)?;");
    eprintln!("        let model  = model.load_record(record);");
    eprintln!("        burn::onnx::export(&model, ...)?;   // <-- burn 버전 별 API");
    eprintln!();
    eprintln!("    현 단계 권장 : ml-py/ 의 Python 경로 사용");
    eprintln!("        cd ml-py && python export_onnx.py --ckpt ckpt.pt --out {:?}", args.out);

    Ok(())
}
