# build.ps1 — 모든 .scad 를 STL 로 렌더 (Windows).
#
# 요구: OpenSCAD 설치 + PATH 등록.
#   - https://openscad.org/downloads.html
#   - 설치 후 `openscad --version` 동작 확인.
#
# 사용:
#   pwsh hardware-3d/build.ps1            # 전체 렌더
#   pwsh hardware-3d/build.ps1 top_deck   # 특정 부품만
#
# 결과: hardware-3d/stl/*.stl (slicer 에 바로 import).

param(
    [string]$Target = "all"
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$StlDir = Join-Path $Here "stl"
if (-not (Test-Path $StlDir)) { New-Item -ItemType Directory -Path $StlDir | Out-Null }

$parts = @("top_deck", "camera_mast", "jetson_tray", "stm32_clip", "cover_shell")
if ($Target -ne "all") { $parts = @($Target) }

# OpenSCAD 위치 자동 탐색
$openscad = $null
foreach ($cand in @(
    "openscad",
    "C:\Program Files\OpenSCAD\openscad.exe",
    "C:\Program Files (x86)\OpenSCAD\openscad.exe"
)) {
    try {
        if ($cand -ne "openscad") {
            if (Test-Path $cand) { $openscad = $cand; break }
        } else {
            Get-Command $cand -ErrorAction Stop | Out-Null
            $openscad = "openscad"; break
        }
    } catch {}
}
if (-not $openscad) { throw "OpenSCAD 를 찾을 수 없음. https://openscad.org/downloads.html 에서 설치하고 PATH 에 등록." }

Push-Location $Here
try {
    foreach ($p in $parts) {
        $scad = "$p.scad"
        $stl  = Join-Path $StlDir "$p.stl"
        Write-Host "[build] $scad -> $stl" -ForegroundColor Cyan
        & $openscad -o $stl $scad
        if ($LASTEXITCODE -ne 0) { throw "OpenSCAD 실패: $scad" }
    }
    Write-Host "[build] 완료. STL 출력물: $StlDir" -ForegroundColor Green
} finally {
    Pop-Location
}
