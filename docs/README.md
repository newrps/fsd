# fsd 가이드 문서

1/10 스케일 자율주행 RC카(`fsd`) 프로젝트의 살아있는 가이드. 코드 변경 시 함께 갱신됩니다.

## 시작 순서 (처음 보는 사람)

1. [setup.md](setup.md) — PC / Jetson / STM32 개발 환경 셋업
2. [hardware.md](hardware.md) — 부품 목록, 핀 매핑, 배선
   - [hardware-setup-checklist.md](hardware-setup-checklist.md) — 첫 조립 step-by-step 체크리스트
   - [../hardware-3d/README.md](../hardware-3d/README.md) — 3D 출력 데크/카메라 마스트/커버 (OpenSCAD)
3. [firmware.md](firmware.md) — STM32 펌웨어 빌드 + 플래시
4. [jetson.md](jetson.md) — Jetson 앱 빌드 + 실행 모드
5. [data-collection.md](data-collection.md) — 학습 데이터 수집
6. [ml-paths.md](ml-paths.md) — ML 학습/추론 세 가지 경로
7. [slam.md](slam.md) — 듀얼 카메라 stereo 깊이/장애물 (스펙 3.2.2)
8. [deployment.md](deployment.md) — 자율주행 배포

## 참고

- [resume-after-parts.md](resume-after-parts.md) — 부품 도착 후 재개 가이드 (단계별 결선 + 명령 + 예상 결과)
- [troubleshooting.md](troubleshooting.md) — 자주 마주치는 문제
- [ci.md](ci.md) — GitHub Actions CI 구성
- [CHANGELOG.md](CHANGELOG.md) — 변경 이력

## 문서 정책

이 문서는 **코드와 함께 갱신**됩니다. 코드만 바꾸고 docs 안 갱신한 PR/커밋은 미완성으로 간주합니다.
- 새 모듈/기능 → 해당 doc 갱신 또는 신규 작성
- 핀/포트/포맷 변경 → `hardware.md` 즉시 반영
- 빌드 명령/feature flag 추가 → `firmware.md` / `jetson.md`
- 매 작업 끝 → `CHANGELOG.md` 한 줄 추가
