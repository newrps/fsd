#!/usr/bin/env bash
# quickstart.sh — fsd ML 파이프라인 1-명령 데모 (Linux/macOS).
#
# 처음 클론한 사람이 5분 안에 결과물을 볼 수 있도록 만든 스크립트.
#   1) ml-py/.venv 생성 + requirements 설치
#   2) pytest 단위테스트
#   3) smoke.py (synthetic → 학습 → ONNX → replay)
#   4) compare_archs.py (PilotNet vs TinyPilotNet)
#   5) notebook_demo.py (시각화 PNG 4장)
#
# 사용:
#   scripts/quickstart.sh                  # 전부 실행
#   scripts/quickstart.sh --skip-compare   # compare_archs 생략 (수 분 절약)
#   scripts/quickstart.sh --skip-demo      # notebook_demo 생략
#   scripts/quickstart.sh --only-tests     # pytest 만

set -euo pipefail

SKIP_COMPARE=0
SKIP_DEMO=0
ONLY_TESTS=0
for arg in "$@"; do
    case "$arg" in
        --skip-compare) SKIP_COMPARE=1 ;;
        --skip-demo)    SKIP_DEMO=1 ;;
        --only-tests)   ONLY_TESTS=1 ;;
        -h|--help)
            sed -n '2,17p' "$0"
            exit 0
            ;;
        *)
            echo "알 수 없는 인자: $arg" >&2
            exit 1
            ;;
    esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ML_PY="$ROOT/ml-py"
VENV="$ML_PY/.venv"
PY="$VENV/bin/python"

export PYTHONIOENCODING=utf-8

echo "[quickstart] root = $ROOT"
cd "$ML_PY"

if [[ ! -x "$PY" ]]; then
    echo "[quickstart] venv 생성 ..."
    python3 -m venv .venv
fi
echo "[quickstart] requirements 설치 ..."
"$PY" -m pip install --upgrade pip --quiet
"$PY" -m pip install -r requirements.txt --quiet

echo "[quickstart] (1/4) pytest ..."
"$PY" -m pytest tests/ -v

if [[ $ONLY_TESTS -eq 1 ]]; then
    echo "[quickstart] --only-tests 지정 — 종료."
    exit 0
fi

echo "[quickstart] (2/4) smoke.py ..."
"$PY" smoke.py

if [[ $SKIP_COMPARE -eq 0 ]]; then
    echo "[quickstart] (3/4) compare_archs.py ..."
    "$PY" compare_archs.py
else
    echo "[quickstart] (3/4) compare_archs 생략 (--skip-compare)"
fi

if [[ $SKIP_DEMO -eq 0 ]]; then
    echo "[quickstart] (4/4) notebook_demo.py ..."
    "$PY" notebook_demo.py
else
    echo "[quickstart] (4/4) notebook_demo 생략 (--skip-demo)"
fi

echo
echo "[quickstart] 완료. 결과물:"
echo "  - ml-py/runs/<arch>/best.pt, model.onnx"
echo "  - recordings/synthetic/  (smoke 합성 데이터)"
echo "  - recordings/demo/*.png  (notebook_demo 시각화)"
