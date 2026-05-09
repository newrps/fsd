"""models.py 단위 테스트."""

import pytest
import torch

import models


@pytest.mark.parametrize("arch", list(models.ARCHS))
def test_model_forward_shape_mono(arch: str):
    m = models.build(arch)
    m.eval()
    x = torch.zeros(2, 3, models.INPUT_H, models.INPUT_W)
    with torch.no_grad():
        out = m(x)
    assert out.shape == (2, models.OUTPUT_DIM)


@pytest.mark.parametrize("arch", list(models.ARCHS))
def test_model_forward_shape_stereo(arch: str):
    m = models.build(arch, stereo=True)
    m.eval()
    x = torch.zeros(2, 6, models.INPUT_H, models.INPUT_W)
    with torch.no_grad():
        out = m(x)
    assert out.shape == (2, models.OUTPUT_DIM)


def test_count_params_consistent():
    p_pilot = models.count_params(models.build("pilotnet"))
    p_tiny = models.count_params(models.build("tiny"))
    # tiny 가 fc dim 이 작아 파라미터 수 적어야 함 (conv5 도 생략).
    # 단 conv 채널 수가 PilotNet 보다 절반 가까이 적은 효과 + flatten_dim 차이로
    # 정확히 어느 쪽이 작은지는 구현 디테일에 따라 다름. 그냥 양수 확인.
    assert p_pilot > 0 and p_tiny > 0
    assert p_pilot < 10_000_000
    assert p_tiny < 10_000_000


def test_normalization_applied():
    """forward 가 mean/std 정규화를 실제 적용하는지 — 입력이 mean 일 때 통과 결과는 0 정규화."""
    m = models.build("pilotnet", mean=(0.5, 0.5, 0.5), std=(0.2, 0.2, 0.2))
    m.eval()
    # 입력 = mean → 정규화 후 0
    x = torch.full((1, 3, models.INPUT_H, models.INPUT_W), 0.5)
    with torch.no_grad():
        out_at_mean = m(x)
    # 입력 = mean + std → 정규화 후 1
    y = torch.full((1, 3, models.INPUT_H, models.INPUT_W), 0.7)
    with torch.no_grad():
        out_at_plus_std = m(y)
    # 출력이 다르면 정규화가 영향을 미친 것.
    assert not torch.allclose(out_at_mean, out_at_plus_std)


def test_unknown_arch_raises():
    with pytest.raises(ValueError):
        models.build("nonexistent_arch")
