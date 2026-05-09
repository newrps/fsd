//! 단계 5: 통신 테스트 — heartbeat + uart_rx + pwm + telemetry + safe_indicator
//!
//! EXTI(RC 캡처) / ADC(배터리) 는 제외. Jetson↔STM32 시리얼만 검증.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, OutputType, Speed};
use embassy_stm32::time::Hertz;
use embassy_stm32::timer::low_level::CountingMode;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm, SimplePwmChannel};
use embassy_stm32::usart::{BufferedUart, BufferedUartRx, BufferedUartTx, Config as UartConfig};
use embassy_stm32::{bind_interrupts, peripherals, usart};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::watch::Watch;
use embassy_time::{Duration, Instant, Timer};
use fsd_protocol::pwm as fsd_pwm;
use fsd_protocol::control as fsd_ctrl;
use fsd_protocol::{decode_frame, encode_frame, DriveCommand, Frame, Telemetry, MAX_FRAME};
use heapless::Vec;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USART3 => usart::BufferedInterruptHandler<peripherals::USART3>;
});

static LAST_CMD: Watch<ThreadModeRawMutex, (DriveCommand, Instant), 2> = Watch::new();
static SAFE: Watch<ThreadModeRawMutex, bool, 2> = Watch::new();
static ENCODER_TICKS: AtomicI32 = AtomicI32::new(0);
static BATTERY_MV: AtomicU32 = AtomicU32::new(0);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    {
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
        config.rcc.mux.adcsel = mux::Adcsel::PER;
    }
    let p = embassy_stm32::init(config);
    info!("step5: communication test (uart_rx + pwm + telemetry)");

    let led_hb = Output::new(p.PB0, Level::Low, Speed::Low);
    let led_safe = Output::new(p.PE1, Level::Low, Speed::Low);

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
    uart_cfg.baudrate = 115_200;
    static TX_BUF: StaticCell<[u8; 256]> = StaticCell::new();
    static RX_BUF: StaticCell<[u8; 512]> = StaticCell::new();
    let tx_buf = TX_BUF.init([0u8; 256]);
    let rx_buf = RX_BUF.init([0u8; 512]);
    let uart = BufferedUart::new(p.USART3, p.PD9, p.PD8, tx_buf, rx_buf, Irqs, uart_cfg)
        .expect("USART3 init");
    let (uart_tx, uart_rx) = uart.split();

    spawner.spawn(heartbeat(led_hb).unwrap());
    spawner.spawn(safe_indicator(led_safe).unwrap());
    spawner.spawn(uart_rx_task(uart_rx).unwrap());
    spawner.spawn(pwm_task(pwm).unwrap());
    spawner.spawn(telemetry_task(uart_tx).unwrap());
    info!("all tasks spawned");
}

#[embassy_executor::task]
async fn heartbeat(mut led: Output<'static>) {
    info!("heartbeat: STARTED");
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

#[embassy_executor::task]
async fn uart_rx_task(mut rx: BufferedUartRx<'static>) {
    info!("uart_rx: STARTED");
    let mut buf = [0u8; 1];
    let mut frame: Vec<u8, MAX_FRAME> = Vec::new();
    let cmd_tx = LAST_CMD.sender();

    loop {
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

#[embassy_executor::task]
async fn pwm_task(mut pwm: SimplePwm<'static, peripherals::TIM3>) {
    let max = pwm.ch1().max_duty_cycle();
    pwm.ch1().enable();
    pwm.ch2().enable();
    info!("PWM max_duty = {}", max);

    info!("ESC arming: {}ms neutral", fsd_ctrl::ESC_ARMING_DURATION_MS);
    SAFE.sender().send(true);
    let arm_until = Instant::now() + Duration::from_millis(fsd_ctrl::ESC_ARMING_DURATION_MS);
    while Instant::now() < arm_until {
        set_pwm_pulse(&mut pwm.ch1(), max, fsd_ctrl::ESC_ARMING_PULSE_US);
        set_pwm_pulse(&mut pwm.ch2(), max, fsd_ctrl::ESC_ARMING_PULSE_US);
        Timer::after(Duration::from_millis(20)).await;
    }
    SAFE.sender().send(false);
    info!("ESC armed");

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
    info!("telemetry: STARTED");
    let mut seq: u32 = 0;
    let mut cmd_rx = LAST_CMD.receiver().expect("LAST_CMD receiver");
    let mut safe_rx = SAFE.receiver().expect("SAFE receiver");

    loop {
        let last_applied = cmd_rx.try_get().map(|(c, _)| c.seq).unwrap_or(0);
        let safe = safe_rx.try_get().unwrap_or(true);
        let now = Instant::now();
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
            rc_steering: f32::NAN,
            rc_throttle: f32::NAN,
            rc_present: false,
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

fn normalized_to_pulse_us(x: f32) -> u32 {
    fsd_pwm::normalized_to_pulse_us(x)
}

fn set_pwm_pulse<T>(ch: &mut SimplePwmChannel<'_, T>, max: u32, pulse_us: u32)
where
    T: embassy_stm32::timer::GeneralInstance4Channel,
{
    ch.set_duty_cycle(fsd_pwm::duty_for_pulse_us(max, pulse_us));
}
