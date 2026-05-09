//! fsd-protocol — Jetson <-> STM32 wire protocol.
//!
//! 양쪽에서 같은 정의를 사용하기 위해 `no_std` 호환으로 작성한다.
//! 직렬화는 `postcard`(varint 기반의 작은 바이너리), 프레이밍은 COBS,
//! 무결성은 CRC-16/IBM(0xA001 polynomial) 사용.
//!
//! 프레임 레이아웃 (wire 위에 흘러가는 바이트):
//!
//! ```text
//!   payload = postcard(Frame)
//!   crc     = CRC16-IBM(payload)            // 2 bytes, little-endian
//!   stuffed = COBS(payload || crc)
//!   wire    = stuffed || 0x00               // 0x00 은 frame delimiter
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

pub mod pwm;
pub mod control;

use serde::{Deserialize, Serialize};

/// 최대 페이로드(직렬화 후) 크기. 실제 프레임은 더 클 수 있으므로 buffer는 여유롭게.
pub const MAX_PAYLOAD: usize = 128;
/// 최대 wire 프레임 크기 (COBS overhead + delimiter 고려).
pub const MAX_FRAME: usize = MAX_PAYLOAD + 4 + 2;

/// Jetson → STM32 명령. `f32` 는 정규화된 -1.0..=1.0 범위.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DriveCommand {
    /// 단조 증가 시퀀스. STM32 측 watchdog용으로도 활용한다.
    pub seq: u32,
    /// 조향. -1.0 = 최대 좌, +1.0 = 최대 우.
    pub steering: f32,
    /// 스로틀. -1.0 = 최대 후진, +1.0 = 최대 전진.
    pub throttle: f32,
    /// 비상정지. true 면 즉시 throttle/steering을 중립으로 강제.
    pub estop: bool,
}

impl DriveCommand {
    pub const NEUTRAL: Self = Self {
        seq: 0,
        steering: 0.0,
        throttle: 0.0,
        estop: false,
    };
}

/// STM32 → Jetson 텔레메트리.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
    pub seq: u32,
    /// 가장 최근에 적용한 DriveCommand의 seq. 명령 적용 확인용.
    pub last_applied_seq: u32,
    /// 보드 mcu의 millis() 카운터.
    pub millis: u32,
    /// 휠 엔코더 펄스(누적). 엔코더 미장착이면 0.
    pub encoder_ticks: i32,
    /// 배터리 전압 (V). 미측정이면 NaN.
    pub battery_v: f32,
    /// MCU 측 watchdog가 발동되어 안전모드인지 여부.
    pub safe_mode: bool,

    // ---- RC 수신기 입력 (옵션) -------------------------------------------
    /// RC 수신기 조향 채널을 PWM 캡처한 정규화 값(-1.0..=1.0). 미장착/미수신이면 NaN.
    pub rc_steering: f32,
    /// RC 수신기 스로틀 채널을 PWM 캡처한 정규화 값(-1.0..=1.0). 미장착/미수신이면 NaN.
    pub rc_throttle: f32,
    /// 최근 100 ms 내에 RC 펄스가 둘 다 정상 수신되었는지.
    pub rc_present: bool,
}

/// 모든 메시지의 봉투. 양방향 모두 같은 enum 사용.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Frame {
    Cmd(DriveCommand),
    Tlm(Telemetry),
    /// 양쪽이 살아있는지 확인하는 핑.
    Ping(u32),
    Pong(u32),
}

// -------------------------------------------------------------------------------------------
// 인코딩 / 디코딩
// -------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    Postcard,
    BufferTooSmall,
    BadCrc,
    Cobs,
    Empty,
}

const CRC: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_IBM_SDLC);

/// `Frame`을 wire 바이트로 인코딩한다. 끝에 `0x00` delimiter가 포함된다.
/// `out`은 최소 `MAX_FRAME` 정도 권장.
pub fn encode_frame(frame: &Frame, out: &mut [u8]) -> Result<usize, CodecError> {
    // 1) postcard 직렬화 (스택 버퍼)
    let mut payload = [0u8; MAX_PAYLOAD];
    let body =
        postcard::to_slice(frame, &mut payload).map_err(|_| CodecError::Postcard)?;
    let body_len = body.len();
    if body_len + 2 > MAX_PAYLOAD {
        return Err(CodecError::BufferTooSmall);
    }
    // 2) CRC append
    let crc = CRC.checksum(&payload[..body_len]);
    payload[body_len..body_len + 2].copy_from_slice(&crc.to_le_bytes());
    let pre_cobs = &payload[..body_len + 2];

    // 3) COBS encode + delimiter
    if out.len() < pre_cobs.len() + 2 {
        return Err(CodecError::BufferTooSmall);
    }
    let stuffed = cobs_encode(pre_cobs, out)?;
    out[stuffed] = 0x00;
    Ok(stuffed + 1)
}

/// 단일 wire 프레임(끝의 `0x00` 포함 또는 미포함)을 디코드한다.
pub fn decode_frame(wire: &[u8]) -> Result<Frame, CodecError> {
    if wire.is_empty() {
        return Err(CodecError::Empty);
    }
    let body_end = wire.iter().position(|&b| b == 0x00).unwrap_or(wire.len());
    let stuffed = &wire[..body_end];
    let mut buf = [0u8; MAX_PAYLOAD + 2];
    let n = cobs_decode(stuffed, &mut buf)?;
    if n < 3 {
        return Err(CodecError::BufferTooSmall);
    }
    let (body, crc_bytes) = buf[..n].split_at(n - 2);
    let crc_got = u16::from_le_bytes([crc_bytes[0], crc_bytes[1]]);
    let crc_want = CRC.checksum(body);
    if crc_got != crc_want {
        return Err(CodecError::BadCrc);
    }
    postcard::from_bytes(body).map_err(|_| CodecError::Postcard)
}

// -------------------------------------------------------------------------------------------
// COBS — Consistent Overhead Byte Stuffing.
// 0x00 byte를 frame delimiter로 쓸 수 있게 페이로드에서 0x00을 제거한다.
// 참고: https://en.wikipedia.org/wiki/Consistent_Overhead_Byte_Stuffing
// -------------------------------------------------------------------------------------------

fn cobs_encode(input: &[u8], out: &mut [u8]) -> Result<usize, CodecError> {
    if out.len() < input.len() + 1 {
        return Err(CodecError::BufferTooSmall);
    }
    let mut code_idx = 0usize;
    let mut write_idx = 1usize;
    let mut code: u8 = 1;
    out[code_idx] = 0;
    for &b in input {
        if b == 0 {
            out[code_idx] = code;
            code = 1;
            code_idx = write_idx;
            out[code_idx] = 0;
            write_idx += 1;
        } else {
            out[write_idx] = b;
            write_idx += 1;
            code += 1;
            if code == 0xFF {
                out[code_idx] = code;
                code = 1;
                code_idx = write_idx;
                if write_idx >= out.len() {
                    return Err(CodecError::BufferTooSmall);
                }
                out[code_idx] = 0;
                write_idx += 1;
            }
        }
    }
    out[code_idx] = code;
    Ok(write_idx)
}

fn cobs_decode(input: &[u8], out: &mut [u8]) -> Result<usize, CodecError> {
    let mut read_idx = 0usize;
    let mut write_idx = 0usize;
    while read_idx < input.len() {
        let code = input[read_idx];
        if code == 0 || (read_idx as usize) + (code as usize) > input.len() {
            return Err(CodecError::Cobs);
        }
        read_idx += 1;
        for _ in 1..code {
            if write_idx >= out.len() {
                return Err(CodecError::BufferTooSmall);
            }
            out[write_idx] = input[read_idx];
            write_idx += 1;
            read_idx += 1;
        }
        if code != 0xFF && read_idx < input.len() {
            if write_idx >= out.len() {
                return Err(CodecError::BufferTooSmall);
            }
            out[write_idx] = 0;
            write_idx += 1;
        }
    }
    Ok(write_idx)
}

// -------------------------------------------------------------------------------------------
// 테스트
// -------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_cmd() {
        let f = Frame::Cmd(DriveCommand {
            seq: 42,
            steering: -0.25,
            throttle: 0.5,
            estop: false,
        });
        let mut buf = [0u8; MAX_FRAME];
        let n = encode_frame(&f, &mut buf).unwrap();
        let out = decode_frame(&buf[..n - 1]).unwrap(); // delimiter 제외
        assert_eq!(f, out);
    }

    #[test]
    fn roundtrip_tlm() {
        let f = Frame::Tlm(Telemetry {
            seq: 1,
            last_applied_seq: 0,
            millis: 1234,
            encoder_ticks: -7,
            battery_v: 11.8,
            safe_mode: false,
            rc_steering: 0.25,
            rc_throttle: -0.10,
            rc_present: true,
        });
        let mut buf = [0u8; MAX_FRAME];
        let n = encode_frame(&f, &mut buf).unwrap();
        let out = decode_frame(&buf[..n]).unwrap();
        assert_eq!(f, out);
    }

    #[test]
    fn bad_crc_rejected() {
        let f = Frame::Ping(7);
        let mut buf = [0u8; MAX_FRAME];
        let n = encode_frame(&f, &mut buf).unwrap();
        // 가운데 바이트 망가뜨림
        if n > 4 {
            buf[2] ^= 0xFF;
        }
        let r = decode_frame(&buf[..n]);
        assert!(matches!(r, Err(CodecError::BadCrc) | Err(CodecError::Cobs) | Err(CodecError::Postcard)));
    }
}
