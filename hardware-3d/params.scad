// params.scad — 단일 진실 원천 (모든 .scad 가 include)
//
// 변경 시 다른 파일 안 건드려도 됨. 측정값이 다르면 여기만 수정.
// 단위: mm.

// ────────────────────────────────────────────────────────────────
// HSP 94118 차체 (1/10 short course) — 사용자 측정값으로 보정 권장
// ────────────────────────────────────────────────────────────────
// 차체에 박혀 있는 4개의 body post 가 deck 의 mounting feet 와 맞물림.
// 자기 차체 분해해서 자/캘리퍼로 측정해서 다음 두 값을 갱신.
chassis_post_spacing_x = 215;   // 앞뒤 post 간 거리
chassis_post_spacing_y = 170;   // 좌우 post 간 거리
chassis_post_diameter  = 6;     // body post 굵기
chassis_post_clip_d    = 1.5;   // body clip 핀 굵기 (수평 관통)
chassis_post_height    = 80;    // chassis 윗면 ~ post 끝 높이

// ────────────────────────────────────────────────────────────────
// 데크 플레이트
// ────────────────────────────────────────────────────────────────
deck_thickness    = 4;          // 4mm PLA = 충분한 강성
deck_margin_x     = 15;         // post 바깥쪽 여유 (앞뒤)
deck_margin_y     = 10;         // post 바깥쪽 여유 (좌우)
deck_corner_r     = 8;          // 모서리 둥글기

deck_size_x = chassis_post_spacing_x + 2 * deck_margin_x;   // 245
deck_size_y = chassis_post_spacing_y + 2 * deck_margin_y;   // 190

// 데크 mounting foot — body post 가 끼는 구멍 + body clip 슬롯
foot_clearance     = 0.4;       // post 와 hole 간격 (PLA 수축 보정)
foot_hole_d        = chassis_post_diameter + foot_clearance;
foot_clip_slot_w   = chassis_post_clip_d + 0.4;
foot_clip_slot_h   = 2.0;       // 슬롯 세로 길이 (post 위아래 위치 조정 여유)

// ────────────────────────────────────────────────────────────────
// Jetson Orin Nano Super Dev Kit
// ────────────────────────────────────────────────────────────────
// 캐리어 보드 (J401/J501 호환 dev kit 기준).
jetson_pcb_x       = 100;
jetson_pcb_y       = 79;
jetson_pcb_z       = 1.6;       // PCB 두께
jetson_hole_dx     = 86;        // 4 모서리 M3 홀 가로 간격
jetson_hole_dy     = 58;        // 세로 간격
jetson_hole_d      = 3.3;       // M3 통과
jetson_standoff_h  = 6;         // PCB 바닥 ~ deck 사이 여유 (heatsink 고려해 deck 윗면에 설치)
jetson_total_z     = 30;        // PCB + heatsink/fan 전체 높이 (충돌 회피용)

// ────────────────────────────────────────────────────────────────
// STM32 NUCLEO-H753ZI
// ────────────────────────────────────────────────────────────────
nucleo_pcb_x       = 140;
nucleo_pcb_y       = 70;
nucleo_pcb_z       = 1.6;
// NUCLEO 보드 4 모서리 M3 마운팅 홀 — 데이터시트 UM2407 figure 6 참조.
// 측정값과 다르면 보정.
nucleo_hole_dx     = 130;
nucleo_hole_dy     = 60;
nucleo_hole_d      = 3.3;
nucleo_standoff_h  = 5;
nucleo_total_z     = 18;        // ST-LINK 부품 포함 위쪽 높이

// ────────────────────────────────────────────────────────────────
// IMX219 카메라 모듈 (Arducam/Waveshare 표준 보드)
// ────────────────────────────────────────────────────────────────
imx219_pcb_x       = 25;
imx219_pcb_y       = 24;
imx219_pcb_z       = 1.0;
imx219_hole_dx     = 21;
imx219_hole_dy     = 12.5;
imx219_hole_d      = 2.4;       // M2 통과 (2.0 + 0.4 클리어런스)
imx219_lens_offset_y = 2.5;     // 렌즈 중심이 PCB 중심에서 살짝 위
imx219_lens_d      = 7.0;       // 렌즈 마운트 외경

// 스테레오 baseline (사용자 선택: 80mm)
camera_baseline    = 80;
camera_tilt_deg    = 8;         // 도로 8° 아래로 향함 (기본값, 슬롯으로 ±15° 조정)

// ────────────────────────────────────────────────────────────────
// 카메라 마스트 (전방 수직 봉)
// ────────────────────────────────────────────────────────────────
mast_height        = 60;        // deck 위에서 60mm
mast_base_thickness = 4;
mast_arm_thickness = 3;
mast_arm_width     = 8;

// ────────────────────────────────────────────────────────────────
// 배터리 (LiPo 2S, 일반적인 5000mAh hard-case 가정)
// ────────────────────────────────────────────────────────────────
battery_x = 140;
battery_y = 47;
battery_z = 25;
battery_strap_w = 12;           // 벨크로 스트랩 슬롯 너비

// ────────────────────────────────────────────────────────────────
// 커버 셸 (RC 바디 대체)
// ────────────────────────────────────────────────────────────────
cover_thickness    = 2.4;       // 얇게 (가벼움 + 빠른 출력)
cover_height       = 95;        // deck 위 95mm — Jetson + 여유
cover_overhang     = 5;         // deck 가장자리 바깥으로 빠지는 양
cover_vent_slot_w  = 4;
cover_vent_slot_l  = 30;

// ────────────────────────────────────────────────────────────────
// 공통
// ────────────────────────────────────────────────────────────────
$fn = 48;                       // 렌더 해상도
m3_nut_w           = 5.6;       // M3 너트 평면 거리 (육각 across-flats)
m3_nut_h           = 2.5;       // M3 너트 두께
m3_head_d          = 6.0;       // M3 캡헤드 외경
m3_head_h          = 3.0;
