"""NUCLEO-H753ZI 마운트 플레이트.

NUCLEO-144 폼팩터 (H753ZI 포함). PCB ≈ 70 x 133 mm. 5개의 M2.5 hole.
※ ST 데이터시트 hole 좌표(보드 origin 기준)를 정확히 가져와야 한다.
   여기서는 대략적인 4-corner 패턴 + 1개 중간으로 두고,
   실측 후 좌표를 보정할 것.
"""

from pathlib import Path
import cadquery as cq

# 보수적 추정 — 실측 후 갱신.
NUCLEO_HOLES = [
    ( -32.5,  -60.0),
    ( +32.5,  -60.0),
    ( -32.5,  +60.0),
    ( +32.5,  +60.0),
    (   0.0,    0.0),
]
HOLE_D = 2.7   # M2.5 관통

CHASSIS_HOLE_PITCH_X = 70.0
CHASSIS_HOLE_PITCH_Y = 40.0
CHASSIS_HOLE_D       = 3.4

PLATE_W = 90.0
PLATE_H = 150.0
PLATE_T = 3.5

STANDOFF_OD = 6.0
STANDOFF_ID = 2.7
STANDOFF_H  = 5.0


def build():
    plate = (
        cq.Workplane("XY")
        .box(PLATE_W, PLATE_H, PLATE_T)
        .edges("|Z").fillet(3.0)
    )
    plate = plate.faces(">Z").workplane().pushPoints(NUCLEO_HOLES).hole(HOLE_D)

    chassis_holes = [
        (-CHASSIS_HOLE_PITCH_X / 2,  PLATE_H / 2 - 8),
        (+CHASSIS_HOLE_PITCH_X / 2,  PLATE_H / 2 - 8),
        (-CHASSIS_HOLE_PITCH_X / 2, -PLATE_H / 2 + 8),
        (+CHASSIS_HOLE_PITCH_X / 2, -PLATE_H / 2 + 8),
    ]
    plate = plate.faces(">Z").workplane().pushPoints(chassis_holes).hole(CHASSIS_HOLE_D)

    for x, y in NUCLEO_HOLES:
        boss = (
            cq.Workplane("XY")
            .center(x, y)
            .circle(STANDOFF_OD / 2)
            .extrude(STANDOFF_H)
            .faces(">Z").workplane().hole(STANDOFF_ID)
            .translate((0, 0, PLATE_T / 2))
        )
        plate = plate.union(boss)

    return plate


def main():
    out = Path(__file__).parent / "build"
    out.mkdir(exist_ok=True)
    part = build()
    cq.exporters.export(part, str(out / "nucleo_mount.stl"))
    cq.exporters.export(part, str(out / "nucleo_mount.step"))
    print(f"wrote {out/'nucleo_mount.stl'}")


if __name__ == "__main__":
    main()
