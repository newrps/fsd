# quickstart.ps1 — fsd ML 파이프라인 1-명령 데모 (Windows PowerShell).
#
# 처음 클론한 사람이 5분 안에 결과물을 볼 수 있도록 만든 스크립트.
#   1) ml-py/.venv 생성 + requirements 설치
#   2) pytest 단위테스트
#   3) smoke.py (synthetic → 학습 → ONNX → replay)
#   4) compare_archs.py (PilotNet vs TinyPilotNet)
#   5) notebook_demo.py (시각화 PNG 4장)
#
# 사용:
#   pwsh scripts/quickstart.ps1            # 전부 실행
#   pwsh scripts/quickstart.ps1 -SkipCompare   # compare_archs 생략 (수 분 절약)
#   pwsh scripts/quickstart.ps1 -SkipDemo      # notebook_demo 생략
#   pwsh scripts/quickstart.ps1 -OnlyTests     # pytest 만

param(
    [switch]$SkipCompare,
    [switch]$SkipDemo,
    [switch]$OnlyTests
)

$ErrorActionPreference = "Stop"
$env:PYTHONIOENCODING = "utf-8"

$Root = Split-Path -Parent $PSScriptRoot
$MlPy = Join-Path $Root "ml-py"
$Venv = Join-Path $MlPy ".venv"
$Py   = Join-Path $Venv "Scripts\python.exe"

Write-Host "[quickstart] root = $Root"
Push-Location $MlPy
try {
    if (-not (Test-Path $Py)) {
        Write-Host "[quickstart] venv 생성 ..." -ForegroundColor Cyan
        python -m venv .venv
    }
    Write-Host "[quickstart] requirements 설치 ..." -ForegroundColor Cyan
    & $Py -m pip install --upgrade pip --quiet
    & $Py -m pip install -r requirements.txt --quiet

    Write-Host "[quickstart] (1/4) pytest ..." -ForegroundColor Yellow
    & $Py -m pytest tests/ -v
    if ($LASTEXITCODE -ne 0) { throw "pytest 실패" }

    if ($OnlyTests) {
        Write-Host "[quickstart] -OnlyTests 지정 — 종료." -ForegroundColor Green
        return
    }

    Write-Host "[quickstart] (2/4) smoke.py ..." -ForegroundColor Yellow
    & $Py smoke.py
    if ($LASTEXITCODE -ne 0) { throw "smoke 실패" }

    if (-not $SkipCompare) {
        Write-Host "[quickstart] (3/4) compare_archs.py ..." -ForegroundColor Yellow
        & $Py compare_archs.py
        if ($LASTEXITCODE -ne 0) { throw "compare_archs 실패" }
    } else {
        Write-Host "[quickstart] (3/4) compare_archs 생략 (-SkipCompare)" -ForegroundColor DarkGray
    }

    if (-not $SkipDemo) {
        Write-Host "[quickstart] (4/4) notebook_demo.py ..." -ForegroundColor Yellow
        & $Py notebook_demo.py
        if ($LASTEXITCODE -ne 0) { throw "notebook_demo 실패" }
    } else {
        Write-Host "[quickstart] (4/4) notebook_demo 생략 (-SkipDemo)" -ForegroundColor DarkGray
    }

    Write-Host ""
    Write-Host "[quickstart] 완료. 결과물:" -ForegroundColor Green
    Write-Host "  - ml-py/runs/<arch>/best.pt, model.onnx"
    Write-Host "  - recordings/synthetic/  (smoke 합성 데이터)"
    Write-Host "  - recordings/demo/*.png  (notebook_demo 시각화)"
}
finally {
    Pop-Location
}
