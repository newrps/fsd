// cover_shell.scad — 풀 커스텀 커버 (기존 RC 바디쉘 대체).
//
// 형상: 둥근 모서리 박스 (사다리꼴 측면, 앞쪽 살짝 낮음).
// 카메라 마스트는 커버 앞쪽 슬롯으로 통과 → 마스트 기둥은 외부에서 보임.
// 옆면/뒷면 환기 슬롯, 위쪽 status LED 창.
//
// 출력 시 큰 부품이라 베드 사이즈 220×220 이상 필요. 분할 안 함.
// PLA 0.3mm 레이어, infill 15%, top/bottom 3 layer 권장.

include <params.scad>;

cover_x = deck_size_x + 2 * cover_overhang;
cover_y = deck_size_y + 2 * cover_overhang;
cover_z = cover_height;

// 앞쪽 절단면 (사다리꼴 윗변이 뒤보다 앞이 짧음)
front_top_drop = 25;            // 앞쪽 윗변이 뒤보다 25mm 낮음

module cover_outer() {
    hull() {
        // 바닥 4 모서리
        for (sx = [-1, 1], sy = [-1, 1])
            translate([sx * (cover_x/2 - deck_corner_r),
                       sy * (cover_y/2 - deck_corner_r), 0])
                cylinder(r=deck_corner_r, h=0.1);
        // 윗면 — 앞쪽이 낮은 사다리꼴
        for (sy = [-1, 1]) {
            translate([(cover_x/2 - deck_corner_r - 5),
                       sy * (cover_y/2 - deck_corner_r - 5),
                       cover_z - front_top_drop])
                sphere(r=deck_corner_r, $fn=24);
            translate([-(cover_x/2 - deck_corner_r - 5),
                       sy * (cover_y/2 - deck_corner_r - 5),
                       cover_z])
                sphere(r=deck_corner_r, $fn=24);
        }
    }
}

module cover_shell() {
    difference() {
        cover_outer();
        // 안쪽 비우기
        translate([0, 0, -0.1])
            scale([(cover_x - 2 * cover_thickness)/cover_x,
                   (cover_y - 2 * cover_thickness)/cover_y,
                   1])
                cover_outer();
        // 바닥 완전 개방 (deck 쪽으로)
        translate([0, 0, -1])
            cube([cover_x + 1, cover_y + 1, 2], center=true);
    }
}

module cover() {
    difference() {
        cover_shell();

        // 1) 카메라 마스트 통과 슬롯 (앞쪽 윗면)
        // 마스트는 deck mast_offset_x 위치에 박혀 있음.
        translate([mast_offset_x, 0, cover_z - front_top_drop - 5])
            cube([40, 30, 50], center=true);

        // 2) 카메라 시야 절단 (앞쪽 벽 비스듬한 면이 카메라 시야를 가리지 않도록)
        translate([cover_x/2 - 10, 0, cover_z - front_top_drop - 25])
            rotate([0, 30, 0])
                cube([30, camera_baseline + 30, 35], center=true);

        // 3) 측면 환기 슬롯 (좌우 각 4줄)
        for (sy = [-1, 1])
            for (i = [-1.5, -0.5, 0.5, 1.5])
                translate([i * 30, sy * (cover_y/2), cover_z/2])
                    rotate([0, 0, 90])
                        cube([cover_vent_slot_l, cover_thickness + 2, cover_vent_slot_w], center=true);

        // 4) 뒤쪽 환기 (Jetson 발열 배출)
        for (i = [-1, 0, 1])
            translate([-cover_x/2, i * 25, cover_z * 0.4])
                rotate([0, 90, 0])
                    cylinder(d=8, h=cover_thickness + 2, center=true);

        // 5) USB-C 전원 액세스 (Jetson 측면)
        translate([jetson_offset_x + 15, cover_y/2, 30])
            cube([20, cover_thickness + 2, 14], center=true);

        // 6) ST-LINK USB 액세스 (NUCLEO 뒤)
        translate([nucleo_offset_x - 30, 0, 25])
            rotate([0, 90, 0])
                cylinder(d=14, h=cover_thickness + 2, center=true);

        // 7) Status LED 창 (윗면, 작은 슬롯)
        translate([0, 0, cover_z - front_top_drop/2])
            cube([6, 25, 5], center=true);

        // 8) deck 모서리 body clip 핀 통과 (4개)
        // 커버는 deck 위에 그냥 놓이고, deck 의 body post 가 커버 안에 숨음 → 별도 잠금 X.
        // 하지만 흔들림 방지로 측면 4곳에 클립 슬롯.
        for (sx = [-1, 1], sy = [-1, 1])
            translate([sx * (deck_size_x/2 - 8), sy * (cover_y/2), 4])
                rotate([90, 0, 0])
                    cylinder(d=4, h=cover_thickness + 2, center=true);
    }
}

cover();
