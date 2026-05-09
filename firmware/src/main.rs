//! NUCLEO-H753ZI 펌웨어 — RC 차량 구동계 제어기.
//!
//! Embassy 0.6 (stm32) / 0.10 (executor) 기반.
//!
//! 책임:
//! 1. Jetson 으로부터 USART3 에 들어오는 `DriveCommand` 프레임을 디코드
//! 2. 두 개의 PWM 채널(서보/조향, ESC/스로틀)을 50 Hz 로 갱신
//! 3. 200 ms 동안 명령이 끊기면 안전모드로 진입(중립 + estop)
//! 4. 50 Hz 로 텔레메트리를 송신
//! 5. RC 수신기 PWM 입력(PB6/PB7)을 µs 정밀도로 캡처해 텔레메트리에 포함
//!
//! 핀 매핑 (NUCLEO-H753ZI 기준):
//! - USART3 TX: PB10  (Zio CN10 D36, 외부 핀헤더 → Jetson J12 pin 10 UART_RX)
//! - USART3 RX: PB11  (Zio CN10 D35, 외부 핀헤더 → Jetson J12 pin 8  UART_TX)
//!   참고: PD8/PD9 는 ST-LINK VCP 전용이지만 NUCLEO-H753ZI MB1364 의 morpho 헤더가
//!   기본 미솔더링이라 외부 접근 불가 → PB10/PB11 (Zio 확장 핀) 사용.
//! - 서보 PWM:  PA6  (TIM3 CH1, 50 Hz, 1000–2000 µs)
//! - ESC PWM:   PA7  (TIM3 CH2, 50 Hz)
//! - RC 조향 in: PA0 (EXTI0, 전용 IRQ — 공유 IRQ EXTI9_5 회피)
//! - RC 스로틀 in: PA1 (EXTI1, 전용 IRQ)
//! - 휠 엔코더 in: PA2 (EXTI2, 전용 IRQ — rising edge 마다 +1)
//! - 배터리 ADC : PC0 (ADC1, 외부 4:1 전압 분배기 가정 — 11.1V LiPo → 2.78V)
//! - LED1:      PB0   (heartbeat)
//! - LED2:      PE1   (safe mode indicator)

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::adc::{Adc, SampleTime};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Level, Output, OutputType, Pull, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::time::Hertz;
use embassy_stm32::timer::low_level::CountingMode;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm, SimplePwmChannel};
use embassy_stm32::usart::{BufferedUart, BufferedUartRx, BufferedUartTx, Config as UartConfig};
use embassy_stm32::{bind_interrupts, exti, peripherals, usart, Peri};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};
use fsd_protocol::pwm as fsd_pwm;
use fsd_protocol::control as fsd_ctrl;
use fsd_protocol::{decode_frame, encode_frame, DriveCommand, Frame, Telemetry, MAX_FRAME};
use heapless::Vec;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

use embassy_stm32::interrupt::typelevel as irq_t;

bind_interrupts!(struct Irqs {
    USART3 => usart::BufferedInterruptHandler<peripherals::USART3>;
    EXTI0  => exti::InterruptHandler<irq_t::EXTI0>;
    EXTI1  => exti::InterruptHandler<irq_t::EXTI1>;
    EXTI2  => exti::InterruptHandler<irq_t::EXTI2>;
});

// ----- 공유 상태 -----------------------------------------------------------

static LAST_CMD: Watch<ThreadModeRawMutex, (DriveCommand, Instant), 2> = Watch::new();
static SAFE: Watch<ThreadModeRawMutex, bool, 2> = Watch::new();
static RC_INPUT: Watch<ThreadModeRawMutex, (f32, f32, Instant), 3> = Watch::new();

/// 휠 엔코더 누적 펄스. rising edge 마다 +1.
static ENCODER_TICKS: AtomicI32 = AtomicI32::new(0);

/// 배터리 전압 (mV). 0 = 미측정.
static BATTERY_MV: AtomicU32 = AtomicU32::new(0);

// ----- main ----------------------------------------------------------------

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    {
        // 240 MHz 안정화 셋업.
        use embassy_stm32::rcc::*;
        config.rcc.hsi = Some(HSIPrescaler::DIV1);
        config.rcc.csi = true;
        config.rcc.hsi48 = Some(Hsi48Config { sync_from_usb: false });
        config.rcc.pll1 = Some(Pll {
            source: PllSource::HSI,
            prediv: PllPreDiv::DIV4,
            mul: PllMul::MUL30,
            divp: Some(PllDiv::DIV2),
            divq: Some(PllDiv::DIV4),
            divr: None,
        });
        config.rcc.sys = Sysclk::PLL1_P;
        config.rcc.ahb_pre = AHBPrescaler::DIV2;
        config.rcc.apb1_pre = APBPrescaler::DIV2;
        config.rcc.apb2_pre = APBPrescaler::DIV2;
        config.rcc.apb3_pre = APBPrescaler::DIV2;
        config.rcc.apb4_pre = APBPrescaler::DIV2;
        config.rcc.voltage_scale = VoltageScale::Scale1;
        // ADC1 은 기본으로 pll2_p 를 쓰는데 우리는 PLL2 미사용. PER(peripheral) 클럭으로 전환.
        // PER 의 소스는 기본 HSI(64MHz) 라 ADC 동작에 충분 (≤80MHz 제약).
        config.rcc.mux.adcsel = mux::Adcsel::PER;
    }
    let p = embassy_stm32::init(config);
    info!("fsd-firmware starting on STM32H753ZI");

    // 디버그: main() 살아있다는 증거로 LD1 켜기.
    let mut led_hb = Output::new(p.PB0, Level::High, Speed::Low);
    info!("LD1 set HIGH (sanity)");
    // 잠깐 켜뒀다가 heartbeat task 가 받아서 토글 시작.
    cortex_m::asm::delay(10_000_000); // ~50ms @ 240MHz
    led_hb.set_low();
    let led_safe = Output::new(p.PE1, Level::Low, Speed::Low);

    // 0.6: PwmPin::new(pin, output_type) — 채널은 핀 타입에서 결정.
    let pwm_ch1 = PwmPin::new(p.PA6, OutputType::PushPull);
    let pwm_ch2 = PwmPin::new(p.PA7, OutputType::PushPull);
    let pwm = SimplePwm::new(
        p.TIM3,
        Some(pwm_ch1),
        Some(pwm_ch2),
        None,
        None,
        Hertz::hz(50),
        CountingMode::EdgeAlignedUp,
    );

    let mut uart_cfg = UartConfig::default();
    uart_cfg.baudrate = 921_600;
    static TX_BUF: StaticCell<[u8; 256]> = StaticCell::new();
    static RX_BUF: StaticCell<[u8; 512]> = StaticCell::new();
    let tx_buf = TX_BUF.init([0u8; 256]);
    let rx_buf = RX_BUF.init([0u8; 512]);
    // 0.6: BufferedUart::new(peri, rx, tx, tx_buf, rx_buf, irq, cfg) — irq 가 buffer 뒤로.
    // PB10 (TX) / PB11 (RX) — Zio CN10 핀헤더로 외부 접근 가능 (PD8/PD9 morpho 미솔더링 회피).
    let uart = BufferedUart::new(p.USART3, p.PB11, p.PB10, tx_buf, rx_buf, Irqs, uart_cfg)
        .expect("USART3 init");
    let (uart_tx, uart_rx) = uart.split();

    // 0.6: ExtiInput::new(pin, ch, pull, irq).
    let rc_steer_pin = ExtiInput::new(p.PA0, p.EXTI0, Pull::Down, Irqs);
    let rc_thr_pin = ExtiInput::new(p.PA1, p.EXTI1, Pull::Down, Irqs);
    let encoder_pin = ExtiInput::new(p.PA2, p.EXTI2, Pull::Up, Irqs);

    info!("spawning tasks...");
    // 0.10: #[task] 매크로가 Result<SpawnToken, SpawnError> 반환. unwrap() 로 토큰만 추출.
    // Spawner::spawn(token) 자체는 () 반환.
    spawner.spawn(heartbeat(led_hb).unwrap());
    info!("1 heartbeat spawned");
    spawner.spawn(safe_indicator(led_safe).unwrap());
    info!("2 safe_indicator spawned");
    spawner.spawn(uart_rx_task(uart_rx).unwrap());
    info!("3 uart_rx spawned");
    spawner.spawn(pwm_task(pwm).unwrap());
    info!("4 pwm_task spawned");
    spawner.spawn(telemetry_task(uart_tx).unwrap());
    info!("5 telemetry_task spawned");
    spawner.spawn(rc_capture_steering(rc_steer_pin).unwrap());
    info!("6 rc_steering spawned");
    spawner.spawn(rc_capture_throttle(rc_thr_pin).unwrap());
    info!("7 rc_throttle spawned");
    spawner.spawn(encoder_task(encoder_pin).unwrap());
    info!("8 encoder spawned");
    // TODO: battery_task 의 ADC blocking_read 가 다른 task 들 starvation 유발 의심.
    // 분배기 회로 결선되면 활성화. 일단 실행 차단.
    let _ = (p.ADC1, p.PC0);
    // spawner.spawn(battery_task(p.ADC1, p.PC0).unwrap());
    info!("MAIN done, executor takes over");
}

// ----- 태스크 정의 ---------------------------------------------------------

#[embassy_executor::task]
async fn heartbeat(mut led: Output<'static>) {
    info!("heartbeat task: STARTED");
    let mut tick: u32 = 0;
    loop {
        led.toggle();
        if tick % 10 == 0 {
            info!("heartbeat tick {}", tick);
        }
        tick = tick.wrapping_add(1);
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[embassy_executor::task]
async fn safe_indicator(mut led: Output<'static>) {
    let mut rx = SAFE.receiver().expect("SAFE receiver");
    loop {
        let v = rx.changed().await;
        led.set_level(if v { Level::High } else { Level::Low });
    }
}

/// COBS 프레임 누적 + 디코드.
#[embassy_executor::task]
async fn uart_rx_task(mut rx: BufferedUartRx<'static>) {
    let mut buf = [0u8; 1];
    let mut frame: Vec<u8, MAX_FRAME> = Vec::new();
    let cmd_tx = LAST_CMD.sender();

    loop {
        // BufferedUartRx 에는 private inherent `read` 가 있어 트레이트 호출은 UFCS 로.
        match embedded_io_async::Read::read(&mut rx, &mut buf).await {
            Ok(_) => {
                let b = buf[0];
                if b == 0x00 {
                    if !frame.is_empty() {
                        match decode_frame(&frame) {
                            Ok(Frame::Cmd(c)) => {
                                trace!("cmd seq={} st={=f32} th={=f32}", c.seq, c.steering, c.throttle);
                                cmd_tx.send((c, Instant::now()));
                            }
                            Ok(Frame::Ping(n)) => debug!("ping {}", n),
                            Ok(_) => {}
                            Err(_) => warn!("frame decode error, len={}", frame.len()),
                        }
                        frame.clear();
                    }
                } else if frame.push(b).is_err() {
                    warn!("frame too long, drop");
                    frame.clear();
                }
            }
            Err(e) => {
                error!("uart rx err: {:?}", e);
                Timer::after(Duration::from_millis(10)).await;
            }
        }
    }
}

/// 50 Hz 로 PWM duty 갱신. 부팅 직후 3 초간 ESC arming 중립 펄스 송출.
#[embassy_executor::task]
async fn pwm_task(mut pwm: SimplePwm<'static, peripherals::TIM3>) {
    let max = pwm.ch1().max_duty_cycle();
    pwm.ch1().enable();
    pwm.ch2().enable();
    info!("PWM max_duty = {}", max);

    // ----- ESC arming -----
    // 대부분 RC ESC 는 부팅 시 일정 시간 동안 중립 펄스를 봐야 정상 arming.
    // 시간/펄스폭은 protocol/control.rs 의 상수로 통일.
    info!("ESC arming: {}ms neutral pulse", fsd_ctrl::ESC_ARMING_DURATION_MS);
    SAFE.sender().send(true);
    let arm_until = Instant::now() + Duration::from_millis(fsd_ctrl::ESC_ARMING_DURATION_MS);
    while Instant::now() < arm_until {
        set_pwm_pulse(&mut pwm.ch1(), max, fsd_ctrl::ESC_ARMING_PULSE_US);
        set_pwm_pulse(&mut pwm.ch2(), max, fsd_ctrl::ESC_ARMING_PULSE_US);
        Timer::after(Duration::from_millis(20)).await;
    }
    SAFE.sender().send(false);
    info!("ESC armed, accepting commands");

    let mut cmd_rx = LAST_CMD.receiver().expect("LAST_CMD receiver");
    let safe_tx = SAFE.sender();

    let mut last_safe = false;
    loop {
        let now = Instant::now();
        let (cmd, ts) = cmd_rx.try_get().unwrap_or((DriveCommand::NEUTRAL, now));
        let age_ms = (now - ts).as_millis();
        let safe = fsd_ctrl::is_safe_mode(&cmd, age_ms, fsd_ctrl::CMD_WATCHDOG_MS);
        let (steer, thr) = fsd_ctrl::resolve_command(&cmd, safe);

        set_pwm_pulse(&mut pwm.ch1(), max, normalized_to_pulse_us(steer));
        set_pwm_pulse(&mut pwm.ch2(), max, normalized_to_pulse_us(thr));

        if safe != last_safe {
            safe_tx.send(safe);
            last_safe = safe;
        }
        Timer::after(Duration::from_millis(20)).await;
    }
}

#[embassy_executor::task]
async fn telemetry_task(mut tx: BufferedUartTx<'static>) {
    let mut seq: u32 = 0;
    let mut cmd_rx = LAST_CMD.receiver().expect("LAST_CMD receiver");
    let mut safe_rx = SAFE.receiver().expect("SAFE receiver");
    let mut rc_rx = RC_INPUT.receiver().expect("RC_INPUT receiver");

    loop {
        let last_applied = cmd_rx.try_get().map(|(c, _)| c.seq).unwrap_or(0);
        let safe = safe_rx.try_get().unwrap_or(true);
        let now = Instant::now();
        let (rc_steering, rc_throttle, rc_present) = match rc_rx.try_get() {
            Some((s, t, ts)) if (now - ts).as_millis() < fsd_ctrl::RC_WATCHDOG_MS => (s, t, true),
            _ => (f32::NAN, f32::NAN, false),
        };
        let battery_mv = BATTERY_MV.load(Ordering::Relaxed);
        let battery_v = if battery_mv > 0 {
            (battery_mv as f32) / 1000.0
        } else {
            f32::NAN
        };
        let tlm = Telemetry {
            seq,
            last_applied_seq: last_applied,
            millis: now.as_millis() as u32,
            encoder_ticks: ENCODER_TICKS.load(Ordering::Relaxed),
            battery_v,
            safe_mode: safe,
            rc_steering,
            rc_throttle,
            rc_present,
        };
        let frame = Frame::Tlm(tlm);
        let mut buf = [0u8; MAX_FRAME];
        match encode_frame(&frame, &mut buf) {
            Ok(n) => {
                if let Err(e) = embedded_io_async::Write::write_all(&mut tx, &buf[..n]).await {
                    error!("uart tx err: {:?}", e);
                }
            }
            Err(_) => warn!("encode err"),
        }
        seq = seq.wrapping_add(1);
        Timer::after(Duration::from_millis(20)).await;
    }
}

#[embassy_executor::task]
async fn rc_capture_steering(mut pin: ExtiInput<'static, Async>) {
    rc_capture_loop(&mut pin, RcChannel::Steering).await;
}

#[embassy_executor::task]
async fn rc_capture_throttle(mut pin: ExtiInput<'static, Async>) {
    rc_capture_loop(&mut pin, RcChannel::Throttle).await;
}

#[derive(Clone, Copy)]
enum RcChannel { Steering, Throttle }

async fn rc_capture_loop(pin: &mut ExtiInput<'static, Async>, which: RcChannel) {
    let tx = RC_INPUT.sender();
    loop {
        pin.wait_for_rising_edge().await;
        let t0 = Instant::now();
        pin.wait_for_falling_edge().await;
        let pulse_us = (Instant::now() - t0).as_micros() as u32;

        if !(800..=2200).contains(&pulse_us) {
            continue;
        }
        let normalized = (((pulse_us as f32) - 1500.0) / 500.0).clamp(-1.0, 1.0);

        let now = Instant::now();
        let (mut s, mut t, _) = RC_INPUT
            .receiver()
            .and_then(|mut r| r.try_get())
            .unwrap_or((0.0, 0.0, now));
        match which {
            RcChannel::Steering => s = normalized,
            RcChannel::Throttle => t = normalized,
        }
        tx.send((s, t, now));
    }
}

/// 휠 엔코더 — rising edge 마다 카운터 증가.
/// 단방향만 (방향 구분은 quadrature 채널 추가 필요).
#[embassy_executor::task]
async fn encoder_task(mut pin: ExtiInput<'static, Async>) {
    loop {
        pin.wait_for_rising_edge().await;
        ENCODER_TICKS.fetch_add(1, Ordering::Relaxed);
    }
}

/// 배터리 전압 측정 — ADC1+PC0, 100 ms 주기.
/// 외부 4:1 전압 분배기 가정 (11.1V LiPo → 2.775V → ADC).
#[embassy_executor::task]
async fn battery_task(
    adc_peri: Peri<'static, peripherals::ADC1>,
    mut pin: Peri<'static, peripherals::PC0>,
) {
    let mut adc = Adc::new(adc_peri);
    // STM32H7 ADC 16-bit, 3.3V ref. 분배기 4배 복원.
    const VREF_MV: u32 = 3300;
    const ADC_FULL_SCALE: u32 = 65535;
    const DIVIDER: u32 = 4;
    loop {
        let raw = adc.blocking_read(&mut pin, SampleTime::CYCLES810_5) as u32;
        let mv = raw * VREF_MV * DIVIDER / ADC_FULL_SCALE;
        BATTERY_MV.store(mv, Ordering::Relaxed);
        Timer::after(Duration::from_millis(100)).await;
    }
}

// ----- 변환 함수 (공유는 fsd_protocol::pwm) --------------------------------

fn normalized_to_pulse_us(x: f32) -> u32 {
    fsd_pwm::normalized_to_pulse_us(x)
}

fn set_pwm_pulse<T>(ch: &mut SimplePwmChannel<'_, T>, max: u32, pulse_us: u32)
where
    T: embassy_stm32::timer::GeneralInstance4Channel,
{
    ch.set_duty_cycle(fsd_pwm::duty_for_pulse_us(max, pulse_us));
}
