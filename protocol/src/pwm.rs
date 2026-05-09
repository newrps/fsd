//! 표준 RC PWM 신호 변환. firmware (출력) 와 jetson (검증·시뮬) 양쪽에서 동일 매핑 보장.
//!
//! 표준 RC 신호:
//!   주기 = 20 000 µs (50 Hz)
//!   1000 µs = 최대 좌 / 후진  (정규화 -1.0)
//!   1500 µs = 중립             (정규화  0.0)
//!   2000 µs = 최대 우 / 전진  (정규화 +1.0)

pub const PERIOD_US: u32 = 20_000;
pub const NEUTRAL_US: u32 = 1500;
pub const HALF_RANGE_US: u32 = 500;
pub const MIN_US: u32 = NEUTRAL_US - HALF_RANGE_US; // 1000
pub const MAX_US: u32 = NEUTRAL_US + HALF_RANGE_US; // 2000

/// 정규화 입력 (-1.0..=1.0) → 펄스폭 (µs).
pub fn normalized_to_pulse_us(x: f32) -> u32 {
    let x = x.clamp(-1.0, 1.0);
    (NEUTRAL_US as f32 + HALF_RANGE_US as f32 * x) as u32
}

/// 펄스폭 (µs) → 정규화 (-1.0..=1.0). 800–2200 µs 범위 밖이면 None (잡음으로 간주).
pub fn pulse_us_to_normalized(pulse_us: u32) -> Option<f32> {
    if !(800..=2200).contains(&pulse_us) {
        return None;
    }
    let n = ((pulse_us as f32) - NEUTRAL_US as f32) / HALF_RANGE_US as f32;
    Some(n.clamp(-1.0, 1.0))
}

/// 펄스폭 (µs) → PWM duty 카운트. `max_duty` 는 타이머의 최대 카운터 값.
pub fn duty_for_pulse_us(max_duty: u32, pulse_us: u32) -> u32 {
    ((max_duty as u64) * (pulse_us as u64) / (PERIOD_US as u64)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints() {
        assert_eq!(normalized_to_pulse_us(-1.0), MIN_US);
        assert_eq!(normalized_to_pulse_us(0.0), NEUTRAL_US);
        assert_eq!(normalized_to_pulse_us(1.0), MAX_US);
    }

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(normalized_to_pulse_us(-2.0), MIN_US);
        assert_eq!(normalized_to_pulse_us(5.0), MAX_US);
    }

    #[test]
    fn roundtrip_normalized() {
        for x100 in -100..=100 {
            let x = (x100 as f32) / 100.0;
            let us = normalized_to_pulse_us(x);
            let back = pulse_us_to_normalized(us).expect("inside range");
            // 정수화 손실로 1% 정도 오차 허용
            assert!((back - x).abs() < 0.011, "x={} us={} back={}", x, us, back);
        }
    }

    #[test]
    fn pulse_out_of_band_rejected() {
        assert!(pulse_us_to_normalized(500).is_none());
        assert!(pulse_us_to_normalized(2500).is_none());
        assert!(pulse_us_to_normalized(800).is_some()); // boundary inclusive
        assert!(pulse_us_to_normalized(2200).is_some());
    }

    #[test]
    fn duty_proportions() {
        // max_duty = 20000 → duty = pulse_us 직결.
        assert_eq!(duty_for_pulse_us(20_000, 1500), 1500);
        // max_duty = 1000 → 50 Hz 인 경우 duty 비율로 환산. 1500/20000 * 1000 = 75
        assert_eq!(duty_for_pulse_us(1_000, 1500), 75);
    }
}
