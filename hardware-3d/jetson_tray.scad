// jetson_tray.scad — Jetson Orin Nano Super Dev Kit standoff 4개.
//
// deck 에 그냥 M3 볼트 + 너트로 직접 고정해도 되지만,
// standoff 가 있으면 (1) 케이블 라우팅 공간 확보, (2) 발열 분리, (3) 진동 흡수 (TPU 출력 시).
//
// 이 파일은 standoff 4개 한 세트. PLA 출력 가능하지만 흡수 위해서는 TPU 권장.

include <params.scad>;

module standoff(h, outer_d=7, hole_d=jetson_hole_d) {
    difference() {
        cylinder(d=outer_d, h=h);
        translate([0, 0, -0.1])
            cylinder(d=hole_d, h=h + 0.2);
        // 위쪽 너트 포켓 (heat-set insert 대신 너트 끼워 넣을 수도 있음)
        translate([0, 0, h - m3_nut_h])
            cylinder(d=m3_nut_w / cos(30), h=m3_nut_h + 0.1, $fn=6);
    }
}

// 4개 동시 출력 (출력 베드 한 번에)
spacing = 12;
for (i = [0:3])
    translate([i * spacing, 0, 0])
        standoff(jetson_standoff_h);
