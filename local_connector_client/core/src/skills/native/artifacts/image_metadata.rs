// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct JpegFrameHeader {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) precision: u8,
    pub(super) components: u8,
    pub(super) segment_length: usize,
    pub(super) frame_end: usize,
}

pub(super) fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 33 || !bytes.starts_with(SIGNATURE) {
        return Err(anyhow!(
            "PNG image has an invalid signature or chunk structure"
        ));
    }
    let mut cursor = SIGNATURE.len();
    let mut dimensions = None;
    let mut chunk_count = 0usize;
    while cursor < bytes.len() {
        chunk_count = chunk_count.saturating_add(1);
        if chunk_count > 100_000 || cursor.saturating_add(12) > bytes.len() {
            return Err(anyhow!("PNG image has an invalid chunk structure"));
        }
        let length = u32::from_be_bytes(bytes[cursor..cursor + 4].try_into()?) as usize;
        let kind = &bytes[cursor + 4..cursor + 8];
        let chunk_end = cursor
            .checked_add(12)
            .and_then(|value| value.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| anyhow!("PNG image has an invalid chunk length"))?;
        if chunk_count == 1 {
            if kind != b"IHDR" || length != 13 {
                return Err(anyhow!("PNG image must begin with a valid IHDR chunk"));
            }
            dimensions = Some((
                u32::from_be_bytes(bytes[cursor + 8..cursor + 12].try_into()?),
                u32::from_be_bytes(bytes[cursor + 12..cursor + 16].try_into()?),
            ));
        }
        if kind == b"IEND" {
            if length != 0 || chunk_end != bytes.len() {
                return Err(anyhow!("PNG image has an invalid terminal IEND chunk"));
            }
            return dimensions.ok_or_else(|| anyhow!("PNG image is missing dimensions"));
        }
        cursor = chunk_end;
    }
    Err(anyhow!("PNG image is missing a terminal IEND chunk"))
}

pub(super) fn jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let header = jpeg_frame_header(bytes)?;
    Ok((header.width, header.height))
}

pub(super) fn jpeg_frame_header(bytes: &[u8]) -> Result<JpegFrameHeader> {
    if bytes.len() < 4 || !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return Err(anyhow!("JPEG image has an invalid start or end marker"));
    }
    let mut cursor = 2usize;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor] != 0xff {
            cursor += 1;
        }
        while cursor < bytes.len() && bytes[cursor] == 0xff {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let marker = bytes[cursor];
        cursor += 1;
        if matches!(marker, 0x01 | 0xd0..=0xd9) {
            continue;
        }
        if cursor + 2 > bytes.len() {
            break;
        }
        let segment_length = usize::from(u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]));
        let frame_end = cursor
            .checked_add(segment_length)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| anyhow!("JPEG image contains an invalid segment length"))?;
        if segment_length < 2 {
            return Err(anyhow!("JPEG image contains an invalid segment length"));
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            if segment_length < 8 {
                return Err(anyhow!("JPEG image has an invalid frame header"));
            }
            return Ok(JpegFrameHeader {
                height: u32::from(u16::from_be_bytes([bytes[cursor + 3], bytes[cursor + 4]])),
                width: u32::from(u16::from_be_bytes([bytes[cursor + 5], bytes[cursor + 6]])),
                precision: bytes[cursor + 2],
                components: bytes[cursor + 7],
                segment_length,
                frame_end,
            });
        }
        if marker == 0xda {
            break;
        }
        cursor = frame_end;
    }
    Err(anyhow!("JPEG image is missing a supported frame header"))
}
