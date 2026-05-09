//! 사람의 운전 명령 입력 소스. 두 가지 백엔드:
//!   - `GamepadSource` : USB 게임패드 (gilrs)         feature = "gamepad"
//!   - `RcSource`      : RC 수신기 → STM32 펌웨어가 PWM 캡처 → 텔레메트리로 전달
//!
//! 두 소스 모두 `InputSource::poll` → `(steering, throttle, estop)` 를 -1.0..=1.0 범위로 반환.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DriveInput {
    pub steering: f32,
    pub throttle: f32,
    pub estop: bool,
    /// 입력이 살아있나(연결됨 + 최근 갱신).
    pub present: bool,
}

impl DriveInput {
    pub const NEUTRAL: Self = Self { steering: 0.0, throttle: 0.0, estop: false, present: false };
}

pub trait InputSource: Send {
    fn poll(&mut self) -> DriveInput;
    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// RC: 텔레메트리에서 수신
// ---------------------------------------------------------------------------

/// 텔레메트리 watch 채널을 구독해 RC 값을 읽는다.
pub struct RcSource {
    rx: tokio::sync::watch::Receiver<DriveInput>,
}

impl RcSource {
    pub fn new(rx: tokio::sync::watch::Receiver<DriveInput>) -> Self {
        Self { rx }
    }
}

impl InputSource for RcSource {
    fn poll(&mut self) -> DriveInput {
        *self.rx.borrow()
    }
    fn name(&self) -> &'static str { "rc" }
}

// ---------------------------------------------------------------------------
// Gamepad: gilrs
// ---------------------------------------------------------------------------

#[cfg(feature = "gamepad")]
pub mod gamepad {
    use super::{DriveInput, InputSource};
    use anyhow::Result;
    use gilrs::{Axis, Button, Gilrs};

    /// 일반적인 Xbox/PS4 매핑:
    ///   - 좌측 스틱 X    : 조향 (-1=좌, +1=우)
    ///   - 우측 트리거    : 전진 (0..1)
    ///   - 좌측 트리거    : 후진 (0..1)
    ///   - South (A/X) 버튼: estop
    pub struct GamepadSource {
        gilrs: Gilrs,
        active: Option<gilrs::GamepadId>,
        last: DriveInput,
        deadzone: f32,
    }

    impl GamepadSource {
        pub fn new() -> Result<Self> {
            let gilrs = Gilrs::new().map_err(|e| anyhow::anyhow!("gilrs init: {:?}", e))?;
            let active = gilrs.gamepads().next().map(|(id, gp)| {
                tracing::info!(name = gp.name(), "gamepad detected");
                id
            });
            if active.is_none() {
                tracing::warn!("no gamepad connected — connect controller and re-run");
            }
            Ok(Self { gilrs, active, last: DriveInput::NEUTRAL, deadzone: 0.05 })
        }
    }

    impl InputSource for GamepadSource {
        fn poll(&mut self) -> DriveInput {
            // 이벤트 큐 비우기 — 최신 상태만 사용.
            while self.gilrs.next_event().is_some() {}

            // 새 컨트롤러가 핫플러그된 경우 픽업.
            if self.active.is_none() {
                self.active = self.gilrs.gamepads().next().map(|(id, _)| id);
            }

            let Some(id) = self.active else {
                self.last = DriveInput::NEUTRAL;
                return self.last;
            };
            let gp = self.gilrs.gamepad(id);

            let steering = apply_deadzone(gp.value(Axis::LeftStickX), self.deadzone);
            let fwd = gp.value(Axis::RightZ).max(0.0);   // RT
            let rev = gp.value(Axis::LeftZ).max(0.0);    // LT
            let throttle = apply_deadzone(fwd - rev, self.deadzone);
            let estop = gp.is_pressed(Button::South);

            self.last = DriveInput { steering, throttle, estop, present: true };
            self.last
        }
        fn name(&self) -> &'static str { "gamepad" }
    }

    fn apply_deadzone(v: f32, dz: f32) -> f32 {
        if v.abs() < dz { 0.0 } else { v }
    }
}
