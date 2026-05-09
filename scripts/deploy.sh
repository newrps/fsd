#!/usr/bin/env bash
# Jetson 으로 소스를 rsync + 원격 빌드 + (선택) 서비스 재시작.
#
# 사용:
#   FSD_HOST=jetson@10.0.0.42 scripts/deploy.sh                    # 빌드만
#   FSD_HOST=jetson@10.0.0.42 scripts/deploy.sh --restart          # 빌드 + systemd restart
#   FSD_HOST=jetson@10.0.0.42 FSD_REMOTE_DIR=/home/jetson/fsd scripts/deploy.sh
#
# 환경변수:
#   FSD_HOST        : SSH 대상 (필수, e.g. jetson@10.0.0.42)
#   FSD_REMOTE_DIR  : 원격 작업 디렉터리 (기본 ~/fsd)
#   FSD_FEATURES    : cargo --features (기본 "camera,gamepad,onnx-tensorrt,slam-opencv")
#   FSD_MODEL       : 같이 보낼 .onnx 파일 (선택)
#   FSD_CALIB       : 같이 보낼 stereo_calib.json (선택)

set -euo pipefail

: "${FSD_HOST:?FSD_HOST 환경변수 필요 (예: jetson@10.0.0.42)}"
REMOTE_DIR="${FSD_REMOTE_DIR:-fsd}"
FEATURES="${FSD_FEATURES:-camera,gamepad,onnx-tensorrt,slam-opencv}"
RESTART=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --restart) RESTART=1; shift ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
echo "==> 원격 디렉터리 준비: $FSD_HOST:$REMOTE_DIR"
ssh "$FSD_HOST" "mkdir -p '$REMOTE_DIR'"

echo "==> rsync 소스 (target/, recordings/, .git 제외)"
rsync -a --info=progress2 \
  --exclude target/ \
  --exclude recordings/ \
  --exclude .git/ \
  --exclude '*.pt' --exclude '*.engine' \
  --exclude ml-py/.venv/ \
  --exclude '__pycache__/' \
  "$REPO_ROOT/" "$FSD_HOST:$REMOTE_DIR/"

if [[ -n "${FSD_MODEL:-}" ]]; then
  echo "==> 모델 전송: $FSD_MODEL"
  scp "$FSD_MODEL" "$FSD_HOST:$REMOTE_DIR/model.onnx"
fi
if [[ -n "${FSD_CALIB:-}" ]]; then
  echo "==> 캘리브레이션 전송: $FSD_CALIB"
  scp "$FSD_CALIB" "$FSD_HOST:$REMOTE_DIR/stereo_calib.json"
fi

echo "==> 원격 빌드 (--features '$FEATURES')"
ssh "$FSD_HOST" bash -lc "'cd $REMOTE_DIR && cargo build --release -p fsd-jetson --features \"$FEATURES\"'"

if [[ "$RESTART" == "1" ]]; then
  echo "==> systemd 서비스 재시작"
  ssh "$FSD_HOST" "sudo systemctl restart fsd-jetson"
  ssh "$FSD_HOST" "sudo systemctl status --no-pager fsd-jetson | head -20"
fi

echo "==> 완료. 직접 실행:"
echo "    ssh $FSD_HOST '$REMOTE_DIR/target/release/fsd-jetson drive --serial /dev/ttyTHS1 --model $REMOTE_DIR/model.onnx --calib $REMOTE_DIR/stereo_calib.json'"
