//! 차량 안전 / 제어 결정 로직. firmware 와 jetson 양쪽이 같은 규칙을 공유한다.
//!
//! firmware 는 이 함수들을 매 50 Hz 루프에서 호출. 호스트에서 단위 테스트로 검증.

use crate::DriveCommand;

/// 명령이 받아 들여질지 판정. 다음 중 하나라도 참이면 NEUTRAL 강제:
///   - `estop` 비트가 켜짐
///   - 마지막 명령이 `watchdog_ms` 이상 오래 됨 (Jetson 끊김)
///
/// 시간은 `now_ms - cmd_ts_ms` 형태로 호출자가 계산해서 넘김
/// (firmware 의 Instant, jetson 의 std::time 둘 다 호환).
pub fn is_safe_mode(cmd: &DriveCommand, age_ms: u64, watchdog_ms: u64) -> bool {
    cmd.estop || age_ms > watchdog_ms
}

/// safe 시 강제 NEUTRAL, 아니면 입력값을 -1.0..=1.0 로 clamp 한 결과.
pub fn resolve_command(cmd: &DriveCommand, safe: bool) -> (f32, f32) {
    if safe {
        (0.0, 0.0)
    } else {
        (cmd.steering.clamp(-1.0, 1.0), cmd.throttle.clamp(-1.0, 1.0))
    }
}

/// ESC arming 단계 펄스폭 (µs). 부팅 시 일정 시간 이 값을 송출해야 ESC 가 정상 arming.
pub const ESC_ARMING_PULSE_US: u32 = crate::pwm::NEUTRAL_US;
pub const ESC_ARMING_DURATION_MS: u64 = 3_000;

/// RC 수신기 watchdog. 마지막 캡처가 너무 오래 되면 RC 미수신.
pub const RC_WATCHDOG_MS: u64 = 100;

/// 명령 watchdog. 마지막 명령이 너무 오래 되면 NEUTRAL.
pub const CMD_WATCHDOG_MS: u64 = 200;

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(steering: f32, throttle: f32, estop: bool) -> DriveCommand {
        DriveCommand { seq: 0, steering, throttle, estop }
    }

    #[test]
    fn fresh_normal_command_is_safe_false() {
        let c = cmd(0.5, 0.3, false);
        assert!(!is_safe_mode(&c, 50, CMD_WATCHDOG_MS));
    }

    #[test]
    fn estop_forces_safe() {
        let c = cmd(0.5, 0.3, true);
        assert!(is_safe_mode(&c, 0, CMD_WATCHDOG_MS));
    }

    #[test]
    fn stale_command_forces_safe() {
        let c = cmd(0.5, 0.3, false);
        assert!(is_safe_mode(&c, CMD_WATCHDOG_MS + 1, CMD_WATCHDOG_MS));
    }

    #[test]
    fn at_watchdog_boundary_not_safe() {
        let c = cmd(0.5, 0.3, false);
        // 정확히 watchdog_ms = 안전 (>가 아닌 >= 면 false). 의도: 200ms 까진 OK.
        assert!(!is_safe_mode(&c, CMD_WATCHDOG_MS, CMD_WATCHDOG_MS));
    }

    #[test]
    fn resolve_safe_returns_neutral() {
        let c = cmd(0.7, -0.3, false);
        let (s, t) = resolve_command(&c, true);
        assert_eq!((s, t), (0.0, 0.0));
    }

    #[test]
    fn resolve_clamps_input() {
        let c = cmd(2.0, -5.0, false);
        let (s, t) = resolve_command(&c, false);
        assert_eq!((s, t), (1.0, -1.0));
    }

    #[test]
    fn resolve_passes_normal() {
        let c = cmd(0.4, -0.2, false);
        let (s, t) = resolve_command(&c, false);
        assert!((s - 0.4).abs() < 1e-6);
        assert!((t - (-0.2)).abs() < 1e-6);
    }
}
