"""drive_sim.CarState bicycle model 단위 테스트."""

import math

from drive_sim import CarState, auto_inputs, render, road_centerline_offset


def test_car_at_rest():
    c = CarState()
    assert c.x == 0.0
    assert c.heading == 0.0
    assert c.speed == 0.0


def test_throttle_accelerates():
    c = CarState()
    for _ in range(60):
        c.step(steering=0.0, throttle=0.5, dt=1 / 30)
    assert c.speed > 0.5, f"60 frame throttle 0.5 면 속도 > 0.5 m/s 여야 함 (got {c.speed})"


def test_drag_decelerates():
    c = CarState()
    c.speed = 3.0
    for _ in range(120):
        c.step(steering=0.0, throttle=0.0, dt=1 / 30)
    assert c.speed < 3.0, "throttle 0 + drag 면 속도 감소"


def test_steering_changes_heading_when_moving():
    c = CarState()
    c.speed = 2.0
    h0 = c.heading
    for _ in range(30):
        c.step(steering=0.5, throttle=0.0, dt=1 / 30)
    assert c.heading != h0
    # 우회전(+) 명령이면 heading +
    assert c.heading > h0


def test_steering_no_effect_at_rest():
    c = CarState()
    h0 = c.heading
    for _ in range(30):
        c.step(steering=1.0, throttle=0.0, dt=1 / 30)
    # 정지 상태에선 heading 안 바뀜 (bicycle model 의 특징).
    assert abs(c.heading - h0) < 1e-6


def test_speed_clamped():
    c = CarState()
    for _ in range(1000):
        c.step(steering=0.0, throttle=1.0, dt=1 / 30)
    assert c.speed <= CarState.MAX_SPEED + 1e-6


def test_auto_inputs_bounded():
    for t in (0.0, 0.5, 1.0, 5.0, 100.0):
        s, th = auto_inputs(t)
        assert -1.0 <= s <= 1.0
        assert -1.0 <= th <= 1.0


def test_road_centerline_periodic():
    c0 = road_centerline_offset(0.0)
    # sin(0) = 0 부근
    assert abs(c0) < 0.01


def test_render_shape_and_dtype():
    import numpy as np
    c = CarState()
    img = render(c, 0.0)
    assert img.shape == (180, 320, 3)
    assert img.dtype == np.uint8
