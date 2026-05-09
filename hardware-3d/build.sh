#!/usr/bin/env bash
# build.sh — 모든 .scad 를 STL 로 렌더 (Linux/macOS).
#
# 요구:
#   sudo apt install openscad     # Ubuntu
#   brew install openscad         # macOS
#
# 사용:
#   ./hardware-3d/build.sh            # 전체
#   ./hardware-3d/build.sh top_deck   # 특정 부품만

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
STL_DIR="$HERE/stl"
mkdir -p "$STL_DIR"

PARTS=(top_deck camera_mast jetson_tray stm32_clip cover_shell)
if [[ $# -gt 0 ]]; then
    PARTS=("$@")
fi

if ! command -v openscad &> /dev/null; then
    echo "openscad 명령 없음. 설치: apt install openscad / brew install openscad" >&2
    exit 1
fi

cd "$HERE"
for p in "${PARTS[@]}"; do
    SCAD="$p.scad"
    STL="$STL_DIR/$p.stl"
    echo "[build] $SCAD -> $STL"
    openscad -o "$STL" "$SCAD"
done
echo "[build] 완료. STL: $STL_DIR"
