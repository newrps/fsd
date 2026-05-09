#!/usr/bin/env bash
# Jetson 위에서 실행 — systemd unit 설치 + 활성화.
#
# 사용:
#   ssh jetson@host
#   cd ~/fsd
#   sudo ./scripts/install_systemd.sh
#   sudo systemctl start fsd-jetson
#   sudo systemctl enable fsd-jetson    # boot 시 자동 시작
#   sudo journalctl -fu fsd-jetson      # 로그 follow

set -euo pipefail

if [[ "$EUID" != "0" ]]; then
  echo "sudo 로 실행하세요" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
UNIT_SRC="$SCRIPT_DIR/fsd-jetson.service"
UNIT_DST="/etc/systemd/system/fsd-jetson.service"

if [[ ! -f "$UNIT_SRC" ]]; then
  echo "유닛 파일 없음: $UNIT_SRC" >&2
  exit 1
fi

echo "==> $UNIT_DST 설치"
cp "$UNIT_SRC" "$UNIT_DST"
chmod 644 "$UNIT_DST"

echo "==> systemctl daemon-reload"
systemctl daemon-reload

echo "==> 다음 단계:"
echo "    sudo systemctl start fsd-jetson      # 즉시 시작"
echo "    sudo systemctl enable fsd-jetson     # boot 자동 시작"
echo "    sudo journalctl -fu fsd-jetson       # 로그"
echo
echo "사전 조건:"
echo "  - /home/jetson/fsd/target/release/fsd-jetson 실행 가능"
echo "  - /home/jetson/fsd/model.onnx 존재"
echo "  - /home/jetson/fsd/stereo_calib.json 존재 (slam-opencv 빌드 시)"
echo "  - jetson 사용자가 dialout 그룹"
