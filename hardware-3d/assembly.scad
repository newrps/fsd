// assembly.scad — 전체 가시화. STL 출력용 아님 (각 부품 .scad 가 STL 마스터).
//
// 이 파일을 OpenSCAD 에서 렌더하면 deck/마스트/Jetson/NUCLEO/cover 가
// 한 좌표계에서 어떻게 맞물리는지 보임. 충돌 검사용.
//
// 사용:
//   openscad assembly.scad        # GUI
//   openscad -o assembly.png assembly.scad     # 렌더만

include <params.scad>;

show_deck   = true;
show_mast   = true;
show_jetson = true;
show_nucleo = true;
show_cover  = false;     // true 면 내부 부품 가림. 외형 확인 시 켤 것.

// ────────────────────────────────────────────────────────────────
// 부품 placeholder (실제 BOM 의 외형 모사 — 충돌 보기 용)
// ────────────────────────────────────────────────────────────────
module jetson_placeholder() {
    color("DarkGreen", 0.9) {
        cube([jetson_pcb_x, jetson_pcb_y, jetson_pcb_z], center=true);
        // heatsink + fan 모사
        translate([0, 0, jetson_pcb_z/2 + (jetson_total_z - jetson_pcb_z)/2])
            color("Silver")
                cube([60, 60, jetson_total_z - jetson_pcb_z - 4], center=true);
        translate([0, 0, jetson_total_z - jetson_pcb_z/2 - 2])
            color("Black")
                cylinder(d=40, h=4, center=true);
    }
}

module nucleo_placeholder() {
    color("DarkGreen", 0.9) {
        cube([nucleo_pcb_x, nucleo_pcb_y, nucleo_pcb_z], center=true);
        // ST-LINK 부분 (한쪽 끝, 두꺼움)
        translate([nucleo_pcb_x/2 - 25, 0, nucleo_pcb_z/2 + 6])
            color("Silver")
                cube([30, 50, 12], center=true);
        // Mini-USB 커넥터 (튀어나옴)
        translate([nucleo_pcb_x/2 + 4, 20, nucleo_pcb_z + 4])
            color("Silver")
                cube([12, 8, 6], center=true);
    }
}

// ────────────────────────────────────────────────────────────────
// 배치 (모든 부품의 좌표계 = deck 중심, deck 윗면 = z=deck_thickness)
// ────────────────────────────────────────────────────────────────
if (show_deck)
    color("LightGray", 0.8)
        import_or_module("top_deck");

if (show_jetson) {
    // standoff 위에 PCB
    translate([25, 0, deck_thickness + jetson_standoff_h + jetson_pcb_z/2])
        jetson_placeholder();
}

if (show_nucleo) {
    translate([-50, 0, deck_thickness + nucleo_standoff_h + nucleo_pcb_z/2])
        nucleo_placeholder();
}

if (show_mast) {
    translate([deck_size_x/2 - 18, 0, deck_thickness])
        color("DimGray", 0.85)
            import_or_module("camera_mast");
}

if (show_cover) {
    color("Coral", 0.4)
        translate([0, 0, deck_thickness])
            import_or_module("cover_shell");
}

// 차체 시뮬레이션 (참고용 wireframe)
%color("Gray", 0.3)
    translate([0, 0, -chassis_post_height/2])
        cube([chassis_post_spacing_x + 60,
              chassis_post_spacing_y + 30,
              chassis_post_height], center=true);

// body post 4개
for (sx = [-1, 1], sy = [-1, 1])
    %color("Gray", 0.4)
        translate([sx * chassis_post_spacing_x/2, sy * chassis_post_spacing_y/2, -2])
            cylinder(d=chassis_post_diameter, h=chassis_post_height + deck_thickness + 5);

// ────────────────────────────────────────────────────────────────
// 헬퍼: 다른 .scad 파일을 module 로 부르는 흉내.
// OpenSCAD 는 include 가 top-level 코드도 실행해버려 까다로움 →
// 여기서는 단순히 import 하지 말고 각 .scad 를 STL 로 미리 렌더해서 import 하거나,
// use <...> 로 모듈만 가져올 것.
// ────────────────────────────────────────────────────────────────
module import_or_module(name) {
    if (name == "top_deck") {
        // 인라인 placeholder — 실제 모양은 top_deck.scad 가 마스터
        translate([-deck_size_x/2, -deck_size_y/2, 0])
            cube([deck_size_x, deck_size_y, deck_thickness]);
    } else if (name == "camera_mast") {
        translate([0, 0, mast_height/2])
            cube([8, camera_baseline + 30, mast_height], center=true);
    } else if (name == "cover_shell") {
        translate([-deck_size_x/2 - 5, -deck_size_y/2 - 5, 0])
            cube([deck_size_x + 10, deck_size_y + 10, cover_height]);
    }
}
