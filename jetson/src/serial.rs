//! STM32 와의 시리얼 브리지. COBS 프레임 단위로 송수신.

use anyhow::{Context, Result};
use bytes::{Buf, BytesMut};
use fsd_protocol::{decode_frame, encode_frame, Frame, MAX_FRAME};
use futures_util::{SinkExt, StreamExt};
use tokio_serial::SerialPortBuilderExt;
use tokio_util::codec::{Decoder, Encoder, Framed};

pub type SerialFramed = Framed<tokio_serial::SerialStream, CobsCodec>;

/// COBS 프레임 코덱 — `0x00` 을 delimiter 로 분리하고 `protocol::decode_frame` 호출.
pub struct CobsCodec;

impl Decoder for CobsCodec {
    type Item = Frame;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>> {
        if let Some(idx) = src.iter().position(|&b| b == 0x00) {
            // [0..idx) = stuffed, idx = delimiter
            let frame_bytes = src.split_to(idx);
            // delimiter 소비
            src.advance(1);
            if frame_bytes.is_empty() {
                return Ok(None);
            }
            match decode_frame(&frame_bytes) {
                Ok(f) => Ok(Some(f)),
                Err(e) => {
                    tracing::warn!(err = ?e, "frame decode error, dropping");
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }
}

impl Encoder<Frame> for CobsCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<()> {
        let mut buf = [0u8; MAX_FRAME];
        let n = encode_frame(&item, &mut buf).map_err(|e| anyhow::anyhow!("encode: {:?}", e))?;
        dst.extend_from_slice(&buf[..n]);
        Ok(())
    }
}

pub async fn open(path: &str, baud: u32) -> Result<SerialFramed> {
    let port = tokio_serial::new(path, baud)
        .timeout(std::time::Duration::from_millis(50))
        .open_native_async()
        .with_context(|| format!("open serial {}", path))?;
    Ok(CobsCodec.framed(port))
}

/// 편의 함수 — 단일 프레임 송신.
pub async fn send(framed: &mut SerialFramed, frame: Frame) -> Result<()> {
    framed.send(frame).await
}

/// 편의 함수 — 다음 프레임 수신.
pub async fn recv(framed: &mut SerialFramed) -> Option<Result<Frame>> {
    framed.next().await
}
