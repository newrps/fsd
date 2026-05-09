"""synthetic.py 단위 테스트."""

import math

import numpy as np

import synthetic


def test_gen_image_shape():
    img = synthetic.gen_image(320, 180, 0.5)
    arr = np.asarray(img)
    assert arr.shape == (180, 320, 3)
    assert arr.dtype == np.uint8


def test_gen_image_peak_position():
    """center_x 가 0.5 면 가장 밝은 픽셀이 가운데. center_x 가 0.2 면 좌측."""
    for cx in (0.2, 0.5, 0.8):
        img = synthetic.gen_image(320, 180, cx)
        arr = np.asarray(img)
        # 가운데 행의 최대 밝기 위치
        row = arr[arr.shape[0] // 2, :, 0].astype(int)
        peak_x = int(np.argmax(row))
        expected_x = int(cx * 320)
        assert abs(peak_x - expected_x) <= 5, f"cx={cx} peak={peak_x} expected~{expected_x}"


def test_gen_image_endpoints_dimmer_than_peak():
    img = synthetic.gen_image(320, 180, 0.5)
    arr = np.asarray(img)
    row = arr[arr.shape[0] // 2, :, 0]
    assert row[160] > row[0]
    assert row[160] > row[-1]
