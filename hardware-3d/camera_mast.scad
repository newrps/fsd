// camera_mast.scad — 스테레오 IMX219 마운트.
//
// 구성:
//   - 베이스 (deck 에 M3×2 로 고정)
//   - 수직 마스트 (높이 mast_height)
//   - 가로 암 (baseline 만큼 좌우로 뻗음)
//   - 양 끝에 카메라 마운트 (틸트 슬롯으로 ±15° 조정)
//
// 스테레오 정렬은 calibration 으로 보정되지만 기계적 변형은 줄이는 게 좋음 →
// PETG 권장, infill 35% 이상.

include <params.scad>;

// ────────────────────────────────────────────────────────────────
// 카메라 한 대 마운트 (틸트 슬롯 포함)
// ────────────────────────────────────────────────────────────────
module camera_plate(mirror=false) {
    plate_x = imx219_pcb_x + 8;        // 33mm
    plate_y = imx219_pcb_y + 6;        // 30mm
    plate_z = 3;

    // 틸트 회전축 위치 (plate 중심)
    difference() {
        union() {
            // 메인 플레이트
            translate([0, 0, plate_z/2])
                cube([plate_x, plate_y, plate_z], center=true);
            // 카메라 PCB 주변 보강 림 (PCB 가 안 흔들리게 1mm 단)
            translate([0, 0, plate_z])
                difference() {
                    cube([imx219_pcb_x + 2, imx219_pcb_y + 2, 1.2], center=true);
                    translate([0, 0, -0.1])
                        cube([imx219_pcb_x + 0.4, imx219_pcb_y + 0.4, 1.4], center=true);
                }
        }

        // IMX219 PCB 4 마운트 홀
        for (sx = [-1, 1], sy = [-1, 1])
            translate([sx * imx219_hole_dx/2, sy * imx219_hole_dy/2, -0.1])
                cylinder(d=imx219_hole_d, h=plate_z + 2);

        // 렌즈 백사이드 빛 통과용 큰 구멍 (PCB 밑면이 보이도록)
        translate([0, imx219_lens_offset_y, -0.1])
            cylinder(d=imx219_lens_d + 2, h=plate_z + 2);
    }
}

// ────────────────────────────────────────────────────────────────
// 카메라 + 틸트 힌지 (가로 암에 붙는 부분)
// ────────────────────────────────────────────────────────────────
module camera_assembly(side_sign) {
    // side_sign: -1 (좌) / +1 (우). 베이스라인 절반만큼 이동.

    translate([0, side_sign * camera_baseline/2, 0]) {
        // 힌지 브래킷 — 가로 암 끝에서 위로 솟은 ㄷ 자
        bracket_h = 20;
        bracket_w = 6;

        difference() {
            translate([0, 0, bracket_h/2])
                cube([bracket_w, 30, bracket_h], center=true);

            // 힌지 핀 홀
            translate([0, 0, bracket_h - 4])
                rotate([0, 90, 0])
                    cylinder(d=jetson_hole_d, h=bracket_w + 2, center=true);

            // 틸트 슬롯 (호 모양 — ±15°)
            for (a = [-15, -10, -5, 0, 5, 10, 15])
                rotate([a, 0, 0])
                    translate([0, 8, bracket_h - 4])
                        rotate([0, 90, 0])
                            cylinder(d=jetson_hole_d, h=bracket_w + 2, center=true);
        }

        // 카메라 플레이트 — 기본 tilt 적용해서 함께 출력 (조립 시 틸트 잠금).
        // 좌/우 둘 다 같은 부호로 아래쪽 기울임 (스테레오 정렬).
        translate([0, 0, bracket_h - 4])
            rotate([camera_tilt_deg, 0, 0])
                translate([0, 8, 0])
                    rotate([0, 0, 90])
                        camera_plate();
    }
}

// ────────────────────────────────────────────────────────────────
// 마스트 본체
// ────────────────────────────────────────────────────────────────
module mast() {
    base_x = 30;
    base_y = 35;

    // 베이스 (deck 에 볼팅)
    difference() {
        translate([0, 0, mast_base_thickness/2])
            cube([base_x, base_y, mast_base_thickness], center=true);
        // M3 마운트 홀 2개 (deck 의 mast_hole_positions 와 일치)
        for (sy = [-1, 1])
            translate([0, sy * 12, -0.1])
                cylinder(d=jetson_hole_d, h=mast_base_thickness + 1);
        // 너트 포켓 (밑면)
        for (sy = [-1, 1])
            translate([0, sy * 12, -0.01])
                cylinder(d=m3_head_d + 0.2, h=m3_head_h, $fn=24);
    }

    // 수직 마스트 — 두 개의 평행 리브 (휨 강성)
    rib_thickness = 4;
    rib_gap = 14;
    for (sx = [-1, 1])
        translate([sx * rib_gap/2, 0, mast_base_thickness])
            cube([rib_thickness, mast_arm_width + 4, mast_height - 8], center=false);
    // 막상 위 두 리브를 연결하는 가로 암
    translate([0, 0, mast_base_thickness + mast_height - 8])
        cube([rib_gap + 2 * rib_thickness, mast_arm_width + 4, mast_arm_thickness], center=true);

    // 가로 암 (좌우로 baseline)
    arm_z = mast_base_thickness + mast_height - 4;
    translate([0, -camera_baseline/2 - 8, arm_z])
        cube([rib_gap + 2 * rib_thickness, camera_baseline + 16, mast_arm_thickness]);

    // 카메라 어셈블리 (좌/우)
    translate([0, 0, arm_z + mast_arm_thickness])
        camera_assembly(-1);
    translate([0, 0, arm_z + mast_arm_thickness])
        camera_assembly(+1);
}

mast();
