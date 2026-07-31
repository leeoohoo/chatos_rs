// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use crc32fast::Hasher as Crc32Hasher;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use lopdf::{dictionary, Document, ObjectId, Stream};
use sha2::{Digest, Sha256};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::image_metadata::jpeg_frame_header;
use super::super::input_file_any;
use super::{
    MAX_PDF_STAMP_DECODED_BYTES, MAX_PDF_STAMP_IMAGE_BYTES, MAX_PDF_STAMP_IMAGE_EDGE,
    MAX_PDF_STAMP_IMAGE_PIXELS,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PdfStampImageFormat {
    Png,
    Jpeg,
}

impl PdfStampImageFormat {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
        }
    }
}

pub(super) struct PdfEmbeddedImage {
    pub(super) relative: String,
    pub(super) format: PdfStampImageFormat,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) source_bytes: usize,
    pub(super) color_space: &'static str,
    pub(super) encoded_color: Vec<u8>,
    pub(super) encoded_alpha: Option<Vec<u8>>,
    pub(super) filter: &'static str,
    pub(super) sha256: String,
}

pub(super) fn pdf_embedded_image(
    state: &LocalState,
    request: &RelayRequest,
    requested: &str,
) -> Result<(PathBuf, PdfEmbeddedImage)> {
    let (path, relative) = input_file_any(state, request, requested)?;
    let metadata = fs::symlink_metadata(path.as_path())
        .with_context(|| format!("inspect PDF stamp image {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(anyhow!(
            "PDF image must be a regular non-symlink workspace file"
        ));
    }
    let bytes =
        fs::read(path.as_path()).with_context(|| format!("read PDF image {}", path.display()))?;
    if bytes.is_empty() || bytes.len() > MAX_PDF_STAMP_IMAGE_BYTES {
        return Err(anyhow!("PDF image must contain between 1 byte and 10 MiB"));
    }
    let source_bytes = bytes.len();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let sha256 = hex::encode(Sha256::digest(bytes.as_slice()));
    let mut image = match extension.as_str() {
        "png" => pdf_embedded_png(bytes.as_slice())?,
        "jpg" | "jpeg" => pdf_embedded_jpeg(bytes)?,
        _ => return Err(anyhow!("PDF image must use .png, .jpg, or .jpeg")),
    };
    image.relative = relative;
    image.sha256 = sha256;
    image.source_bytes = source_bytes;
    Ok((path, image))
}

pub(super) fn add_pdf_embedded_image(
    document: &mut Document,
    image: &PdfEmbeddedImage,
) -> Result<ObjectId> {
    let soft_mask_id = image.encoded_alpha.as_ref().map(|alpha| {
        document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(image.width),
                "Height" => i64::from(image.height),
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
                "Filter" => "FlateDecode",
                "Interpolate" => false,
            },
            alpha.clone(),
        ))
    });
    let mut dictionary = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(image.width),
        "Height" => i64::from(image.height),
        "ColorSpace" => image.color_space,
        "BitsPerComponent" => 8,
        "Filter" => image.filter,
        "Interpolate" => true,
    };
    if let Some(soft_mask_id) = soft_mask_id {
        dictionary.set("SMask", soft_mask_id);
    }
    Ok(document.add_object(Stream::new(dictionary, image.encoded_color.clone())))
}

fn pdf_embedded_png(bytes: &[u8]) -> Result<PdfEmbeddedImage> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 33 || !bytes.starts_with(PNG_SIGNATURE) {
        return Err(anyhow!("PNG image has an invalid signature"));
    }
    let mut cursor = PNG_SIGNATURE.len();
    let mut width = 0_u32;
    let mut height = 0_u32;
    let mut color_type = 0_u8;
    let mut channels = 0_usize;
    let mut saw_ihdr = false;
    let mut saw_idat = false;
    let mut saw_iend = false;
    let mut idat = Vec::new();
    while cursor < bytes.len() {
        if cursor + 12 > bytes.len() {
            return Err(anyhow!("PNG image contains a truncated chunk"));
        }
        let length = usize::try_from(u32::from_be_bytes(bytes[cursor..cursor + 4].try_into()?))
            .context("PNG chunk length exceeds the local platform")?;
        let chunk_type = &bytes[cursor + 4..cursor + 8];
        let data_start = cursor + 8;
        let data_end = data_start
            .checked_add(length)
            .filter(|end| {
                end.checked_add(4)
                    .is_some_and(|crc_end| crc_end <= bytes.len())
            })
            .context("PNG image contains an invalid chunk length")?;
        let crc_end = data_end + 4;
        let expected_crc = u32::from_be_bytes(bytes[data_end..crc_end].try_into()?);
        let mut crc = Crc32Hasher::new();
        crc.update(chunk_type);
        crc.update(&bytes[data_start..data_end]);
        if crc.finalize() != expected_crc {
            return Err(anyhow!("PNG image contains an invalid chunk CRC"));
        }
        match chunk_type {
            b"IHDR" => {
                if saw_ihdr || cursor != PNG_SIGNATURE.len() || length != 13 {
                    return Err(anyhow!("PNG image has an invalid IHDR chunk"));
                }
                width = u32::from_be_bytes(bytes[data_start..data_start + 4].try_into()?);
                height = u32::from_be_bytes(bytes[data_start + 4..data_start + 8].try_into()?);
                let bit_depth = bytes[data_start + 8];
                color_type = bytes[data_start + 9];
                channels = match color_type {
                    0 => 1,
                    2 => 3,
                    4 => 2,
                    6 => 4,
                    _ => {
                        return Err(anyhow!(
                            "PNG PDF images support only 8-bit grayscale, RGB, grayscale-alpha, or RGBA"
                        ));
                    }
                };
                if bit_depth != 8
                    || bytes[data_start + 10] != 0
                    || bytes[data_start + 11] != 0
                    || bytes[data_start + 12] != 0
                {
                    return Err(anyhow!(
                        "PNG PDF images must use 8-bit non-interlaced standard compression"
                    ));
                }
                validate_pdf_stamp_image_dimensions(width, height)?;
                saw_ihdr = true;
            }
            b"IDAT" => {
                if !saw_ihdr || saw_iend {
                    return Err(anyhow!("PNG image has IDAT chunks in an invalid position"));
                }
                saw_idat = true;
                if idat.len().saturating_add(length) > MAX_PDF_STAMP_IMAGE_BYTES {
                    return Err(anyhow!("PNG compressed image data exceeds 10 MiB"));
                }
                idat.extend_from_slice(&bytes[data_start..data_end]);
            }
            b"IEND" => {
                if !saw_ihdr || !saw_idat || saw_iend || length != 0 || crc_end != bytes.len() {
                    return Err(anyhow!("PNG image has an invalid terminal IEND chunk"));
                }
                saw_iend = true;
            }
            b"PLTE" => {
                if !saw_ihdr || saw_idat {
                    return Err(anyhow!("PNG image has a misplaced PLTE chunk"));
                }
            }
            _ if chunk_type[0].is_ascii_uppercase() => {
                return Err(anyhow!("PNG image contains an unsupported critical chunk"));
            }
            _ => {}
        }
        cursor = crc_end;
    }
    if !saw_iend {
        return Err(anyhow!("PNG image is missing a valid terminal IEND chunk"));
    }
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(channels))
        .context("PNG row size exceeds local limits")?;
    let expected_inflated = row_bytes
        .checked_add(1)
        .and_then(|row| row.checked_mul(height as usize))
        .filter(|bytes| *bytes <= MAX_PDF_STAMP_DECODED_BYTES)
        .context("PNG decoded image exceeds the 64 MiB safety limit")?;
    let mut decoder = ZlibDecoder::new(idat.as_slice()).take((expected_inflated + 1) as u64);
    let mut filtered = Vec::with_capacity(expected_inflated);
    decoder
        .read_to_end(&mut filtered)
        .context("decode PNG image data")?;
    if filtered.len() != expected_inflated {
        return Err(anyhow!(
            "PNG decoded image size does not match IHDR dimensions"
        ));
    }
    let decoded = unfilter_png_rows(filtered.as_slice(), row_bytes, height as usize, channels)?;
    let (colors, color_bytes, alpha_bytes) = split_png_channels(decoded.as_slice(), color_type)?;
    Ok(PdfEmbeddedImage {
        relative: String::new(),
        format: PdfStampImageFormat::Png,
        width,
        height,
        source_bytes: bytes.len(),
        color_space: if colors == 1 {
            "DeviceGray"
        } else {
            "DeviceRGB"
        },
        encoded_color: zlib_compress(color_bytes.as_slice())?,
        encoded_alpha: alpha_bytes
            .as_ref()
            .map(|bytes| zlib_compress(bytes.as_slice()))
            .transpose()?,
        filter: "FlateDecode",
        sha256: String::new(),
    })
}

fn pdf_embedded_jpeg(bytes: Vec<u8>) -> Result<PdfEmbeddedImage> {
    let header = jpeg_frame_header(bytes.as_slice())?;
    if header.precision != 8 {
        return Err(anyhow!("JPEG image must use an 8-bit frame header"));
    }
    let minimum_frame_length = 8_usize.saturating_add(usize::from(header.components) * 3);
    if header.segment_length < minimum_frame_length {
        return Err(anyhow!("JPEG image has an incomplete component table"));
    }
    if header.frame_end > bytes.len() - 2
        || !bytes[header.frame_end..bytes.len() - 2]
            .windows(2)
            .any(|marker| marker == [0xff, 0xda])
    {
        return Err(anyhow!("JPEG image is missing a scan header"));
    }
    validate_pdf_stamp_image_dimensions(header.width, header.height)?;
    let color_space = match header.components {
        1 => "DeviceGray",
        3 => "DeviceRGB",
        _ => {
            return Err(anyhow!(
                "JPEG PDF images support only grayscale or RGB color components"
            ));
        }
    };
    let source_bytes = bytes.len();
    Ok(PdfEmbeddedImage {
        relative: String::new(),
        format: PdfStampImageFormat::Jpeg,
        width: header.width,
        height: header.height,
        source_bytes,
        color_space,
        encoded_color: bytes,
        encoded_alpha: None,
        filter: "DCTDecode",
        sha256: String::new(),
    })
}

fn validate_pdf_stamp_image_dimensions(width: u32, height: u32) -> Result<()> {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > MAX_PDF_STAMP_IMAGE_EDGE
        || height > MAX_PDF_STAMP_IMAGE_EDGE
        || pixels > MAX_PDF_STAMP_IMAGE_PIXELS
    {
        return Err(anyhow!(
            "PDF stamp image dimensions exceed the 10000 px edge or 16 megapixel safety limit"
        ));
    }
    Ok(())
}

fn unfilter_png_rows(
    filtered: &[u8],
    row_bytes: usize,
    height: usize,
    bytes_per_pixel: usize,
) -> Result<Vec<u8>> {
    let mut decoded = vec![0_u8; row_bytes.saturating_mul(height)];
    let mut input_offset = 0_usize;
    for row in 0..height {
        let filter = filtered[input_offset];
        input_offset += 1;
        let output_offset = row * row_bytes;
        for column in 0..row_bytes {
            let raw = filtered[input_offset + column];
            let left = if column >= bytes_per_pixel {
                decoded[output_offset + column - bytes_per_pixel]
            } else {
                0
            };
            let up = if row > 0 {
                decoded[output_offset + column - row_bytes]
            } else {
                0
            };
            let up_left = if row > 0 && column >= bytes_per_pixel {
                decoded[output_offset + column - row_bytes - bytes_per_pixel]
            } else {
                0
            };
            let predictor = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth_predictor(left, up, up_left),
                _ => return Err(anyhow!("PNG image uses an unsupported row filter")),
            };
            decoded[output_offset + column] = raw.wrapping_add(predictor);
        }
        input_offset += row_bytes;
    }
    Ok(decoded)
}

fn paeth_predictor(left: u8, up: u8, up_left: u8) -> u8 {
    let left = i32::from(left);
    let up = i32::from(up);
    let up_left = i32::from(up_left);
    let estimate = left + up - up_left;
    let left_distance = (estimate - left).abs();
    let up_distance = (estimate - up).abs();
    let diagonal_distance = (estimate - up_left).abs();
    if left_distance <= up_distance && left_distance <= diagonal_distance {
        left as u8
    } else if up_distance <= diagonal_distance {
        up as u8
    } else {
        up_left as u8
    }
}

fn split_png_channels(decoded: &[u8], color_type: u8) -> Result<(usize, Vec<u8>, Option<Vec<u8>>)> {
    match color_type {
        0 => Ok((1, decoded.to_vec(), None)),
        2 => Ok((3, decoded.to_vec(), None)),
        4 => {
            let mut colors = Vec::with_capacity(decoded.len() / 2);
            let mut alpha = Vec::with_capacity(decoded.len() / 2);
            for pixel in decoded.chunks_exact(2) {
                colors.push(pixel[0]);
                alpha.push(pixel[1]);
            }
            Ok((1, colors, Some(alpha)))
        }
        6 => {
            let mut colors = Vec::with_capacity(decoded.len() / 4 * 3);
            let mut alpha = Vec::with_capacity(decoded.len() / 4);
            for pixel in decoded.chunks_exact(4) {
                colors.extend_from_slice(&pixel[..3]);
                alpha.push(pixel[3]);
            }
            Ok((3, colors, Some(alpha)))
        }
        _ => Err(anyhow!("PNG image uses an unsupported color type")),
    }
}

fn zlib_compress(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .context("compress PDF stamp image data")?;
    encoder.finish().context("finish PDF stamp image data")
}
