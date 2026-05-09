// top_deck.scad — 메인 데크 플레이트.
//
// 차체 body post 4개에 끼워서 고정, 그 위에 Jetson/STM32 가 올라감.
// 큰 컷아웃 없이 단단하게 — 무게보다 강성/정렬 우선 (실차 충격 견디게).

include <params.scad>;

// ────────────────────────────────────────────────────────────────
// 헬퍼: 둥근 사각형 (2D)
// ────────────────────────────────────────────────────────────────
module rounded_rect(x, y, r) {
    hull() {
        for (sx = [-1, 1], sy = [-1, 1])
            translate([sx * (x/2 - r), sy * (y/2 - r)])
                circle(r=r);
    }
}

// ────────────────────────────────────────────────────────────────
// body post 4개 위치 (deck 좌표계 — 중심 원점)
// ────────────────────────────────────────────────────────────────
module post_positions() {
    for (sx = [-1, 1], sy = [-1, 1])
        translate([sx * chassis_post_spacing_x/2, sy * chassis_post_spacing_y/2, 0])
            children();
}

// Jetson 마운트 홀 4개 (deck 위쪽에 설치, 중심에서 약간 앞쪽)
jetson_offset_x = 25;   // deck 중심에서 +x 쪽 (앞으로). 사용자가 조정
module jetson_hole_positions() {
    translate([jetson_offset_x, 0, 0])
        for (sx = [-1, 1], sy = [-1, 1])
            translate([sx * jetson_hole_dx/2, sy * jetson_hole_dy/2, 0])
                children();
}

// STM32 마운트 홀 4개 (deck 뒤쪽). NUCLEO 는 길이 140 이라 가로 배치.
nucleo_offset_x = -50;  // 뒤쪽
module nucleo_hole_positions() {
    translate([nucleo_offset_x, 0, 0])
        for (sx = [-1, 1], sy = [-1, 1])
            translate([sx * nucleo_hole_dx/2, sy * nucleo_hole_dy/2, 0])
                children();
}

// 카메라 마스트 마운트 홀 (deck 가장 앞쪽, 좌우 1쌍)
mast_offset_x = deck_size_x/2 - 18;
module mast_hole_positions() {
    for (sy = [-1, 1])
        translate([mast_offset_x, sy * 12, 0])
            children();
}

// ────────────────────────────────────────────────────────────────
// 메인 모듈
// ────────────────────────────────────────────────────────────────
module top_deck() {
    difference() {
        // 본체
        linear_extrude(deck_thickness)
            rounded_rect(deck_size_x, deck_size_y, deck_corner_r);

        // 1) body post 통과 홀
        post_positions()
            translate([0, 0, -0.1])
                cylinder(d=foot_hole_d, h=deck_thickness + 0.2);

        // 2) body clip 슬롯 (post 측면 핀이 빠질 수 있게)
        post_positions() {
            translate([0, 0, deck_thickness/2])
                rotate([90, 0, 0])
                    linear_extrude(50, center=true)
                        square([foot_clip_slot_w, foot_clip_slot_h], center=true);
        }

        // 3) Jetson 마운트 홀
        jetson_hole_positions()
            translate([0, 0, -0.1])
                cylinder(d=jetson_hole_d, h=deck_thickness + 0.2);

        // 4) Jetson 너트 포켓 (밑면, 헤드 잠김)
        jetson_hole_positions()
            translate([0, 0, -0.01])
                cylinder(d=m3_head_d + 0.2, h=m3_head_h, $fn=24);

        // 5) STM32 마운트 홀
        nucleo_hole_positions()
            translate([0, 0, -0.1])
                cylinder(d=nucleo_hole_d, h=deck_thickness + 0.2);
        nucleo_hole_positions()
            translate([0, 0, -0.01])
                cylinder(d=m3_head_d + 0.2, h=m3_head_h, $fn=24);

        // 6) 카메라 마스트 베이스 마운트 홀
        mast_hole_positions()
            translate([0, 0, -0.1])
                cylinder(d=jetson_hole_d, h=deck_thickness + 0.2);
        mast_hole_positions()
            translate([0, 0, -0.01])
                cylinder(d=m3_head_d + 0.2, h=m3_head_h, $fn=24);

        // 7) 케이블 패스스루 슬롯 (Jetson <-> STM32 사이)
        translate([(jetson_offset_x + nucleo_offset_x)/2, 0, deck_thickness/2])
            cube([15, 30, deck_thickness + 0.2], center=true);

        // 8) 측면 케이블 슬롯 (카메라 CSI / GPIO)
        for (sy = [-1, 1])
            translate([jetson_offset_x, sy * 50, deck_thickness/2])
                cube([20, 8, deck_thickness + 0.2], center=true);

        // 9) 무게 절감 — Jetson/STM32 영역 사이 격자 컷
        // (간단한 슬롯 4개. 너무 많이 비우면 강성 떨어짐)
        for (sx = [-1, 1], sy = [-1, 1])
            translate([sx * 70, sy * 60, deck_thickness/2])
                rotate([0, 0, 90])
                    cube([15, 8, deck_thickness + 0.2], center=true);
    }

    // 마운팅 풋 — post 가 들어가는 짧은 보강 슬리브 (deck 밑면)
    post_positions() {
        translate([0, 0, -3])
            difference() {
                cylinder(d=chassis_post_diameter + 4, h=3);
                translate([0, 0, -0.1])
                    cylinder(d=foot_hole_d, h=4);
            }
    }
}

top_deck();
