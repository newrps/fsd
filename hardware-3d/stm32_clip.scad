// stm32_clip.scad — STM32 NUCLEO-H753ZI standoff + USB 커넥터 보호 클립.
//
// NUCLEO 의 Mini-USB (ST-LINK) 커넥터에 케이블이 자주 빠지면 USART3 RX/TX 가 쉽게 끊김.
// 작은 ㄷ자 클립이 USB 케이블을 잡아주는 역할.

include <params.scad>;

module standoff(h, outer_d=7, hole_d=nucleo_hole_d) {
    difference() {
        cylinder(d=outer_d, h=h);
        translate([0, 0, -0.1])
            cylinder(d=hole_d, h=h + 0.2);
        translate([0, 0, h - m3_nut_h])
            cylinder(d=m3_nut_w / cos(30), h=m3_nut_h + 0.1, $fn=6);
    }
}

module usb_strain_relief() {
    // ST-LINK USB 커넥터 옆에 붙여 케이블을 끼워 두는 ㄷ자 클립.
    // Mini-USB B 플러그 외형 ~9 × 7mm. 살짝 여유.
    body_x = 14;
    body_y = 16;
    body_z = 9;
    wall = 2;

    difference() {
        cube([body_x, body_y, body_z]);
        translate([wall, wall, -0.1])
            cube([body_x - 2*wall, body_y - 2*wall, body_z + 0.2]);
        // 입구 (케이블이 들어가는 쪽)
        translate([wall, -0.1, 2])
            cube([body_x - 2*wall, wall + 0.2, body_z]);
        // 데크 고정용 M3 홀 1개 (밑면)
        translate([body_x/2, body_y/2, -0.1])
            cylinder(d=jetson_hole_d, h=body_z + 0.2);
    }
}

// 4개 standoff + 1개 strain relief 한꺼번에 출력
for (i = [0:3])
    translate([i * 12, 0, 0])
        standoff(nucleo_standoff_h);

translate([0, 20, 0])
    usb_strain_relief();
