const VIDEO_TAIL_ERROR: &str = "视频已保存，但暂时无法提取成片尾帧。";

pub(super) fn last_frame_png(video_bytes: &[u8]) -> Result<Vec<u8>, String> {
    if video_bytes.is_empty() {
        return Err(VIDEO_TAIL_ERROR.into());
    }
    platform::decode_last_frame(video_bytes).and_then(encode_png)
}

struct BgraFrame {
    width: u32,
    height: u32,
    stride: i32,
    rotation: u32,
    bytes: Vec<u8>,
}

fn encode_png(frame: BgraFrame) -> Result<Vec<u8>, String> {
    let (width, height, rgba) = normalized_rgba(&frame)?;
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        writer
            .write_image_data(&rgba)
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
    }
    if png.is_empty() {
        return Err(VIDEO_TAIL_ERROR.into());
    }
    Ok(png)
}

fn normalized_rgba(frame: &BgraFrame) -> Result<(u32, u32, Vec<u8>), String> {
    if frame.width == 0 || frame.height == 0 {
        return Err(VIDEO_TAIL_ERROR.into());
    }
    let stride = if frame.stride == 0 {
        frame
            .width
            .checked_mul(4)
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| VIDEO_TAIL_ERROR.to_owned())?
    } else {
        frame.stride
    };
    let row_bytes = usize::try_from(frame.width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| VIDEO_TAIL_ERROR.to_owned())?;
    let pitch = usize::try_from(stride.unsigned_abs()).map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
    if pitch < row_bytes {
        return Err(VIDEO_TAIL_ERROR.into());
    }
    let required = pitch
        .checked_mul(usize::try_from(frame.height).map_err(|_| VIDEO_TAIL_ERROR.to_owned())?)
        .ok_or_else(|| VIDEO_TAIL_ERROR.to_owned())?;
    if frame.bytes.len() < required {
        return Err(VIDEO_TAIL_ERROR.into());
    }

    let pixel_count = usize::try_from(frame.width)
        .ok()
        .and_then(|width| {
            usize::try_from(frame.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| VIDEO_TAIL_ERROR.to_owned())?;
    let mut upright = vec![0_u8; pixel_count.saturating_mul(4)];
    for y in 0..usize::try_from(frame.height).map_err(|_| VIDEO_TAIL_ERROR.to_owned())? {
        let source_y = if stride < 0 {
            usize::try_from(frame.height).map_err(|_| VIDEO_TAIL_ERROR.to_owned())? - 1 - y
        } else {
            y
        };
        let source_row = source_y
            .checked_mul(pitch)
            .ok_or_else(|| VIDEO_TAIL_ERROR.to_owned())?;
        for x in 0..usize::try_from(frame.width).map_err(|_| VIDEO_TAIL_ERROR.to_owned())? {
            let source = source_row + x * 4;
            let target = (y * usize::try_from(frame.width).unwrap_or_default() + x) * 4;
            upright[target] = frame.bytes[source + 2];
            upright[target + 1] = frame.bytes[source + 1];
            upright[target + 2] = frame.bytes[source];
            upright[target + 3] = 255;
        }
    }

    rotate_rgba(frame.width, frame.height, upright, frame.rotation)
}

fn rotate_rgba(
    width: u32,
    height: u32,
    source: Vec<u8>,
    rotation: u32,
) -> Result<(u32, u32, Vec<u8>), String> {
    let (target_width, target_height) = match rotation {
        0 | 180 => (width, height),
        90 | 270 => (height, width),
        _ => return Err(VIDEO_TAIL_ERROR.into()),
    };
    if rotation == 0 {
        return Ok((width, height, source));
    }
    let source_width = usize::try_from(width).map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
    let source_height = usize::try_from(height).map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
    let target_width_usize =
        usize::try_from(target_width).map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
    let mut target = vec![0_u8; source.len()];
    for y in 0..source_height {
        for x in 0..source_width {
            let (target_x, target_y) = match rotation {
                90 => (source_height - 1 - y, x),
                180 => (source_width - 1 - x, source_height - 1 - y),
                270 => (y, source_width - 1 - x),
                _ => unreachable!("validated rotation"),
            };
            let source_offset = (y * source_width + x) * 4;
            let target_offset = (target_y * target_width_usize + target_x) * 4;
            target[target_offset..target_offset + 4]
                .copy_from_slice(&source[source_offset..source_offset + 4]);
        }
    }
    Ok((target_width, target_height, target))
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{BgraFrame, VIDEO_TAIL_ERROR};
    use std::{ffi::OsStr, io::Write, os::windows::ffi::OsStrExt, path::Path};
    use windows::{
        core::{GUID, PCWSTR},
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            Media::MediaFoundation::{
                IMFMediaBuffer, IMFSample, MFCreateAttributes, MFCreateMediaType,
                MFCreateSourceReaderFromURL, MFMediaType_Video, MFShutdown, MFStartup,
                MFVideoFormat_RGB32, MFSTARTUP_FULL, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE,
                MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_MT_VIDEO_ROTATION, MF_PD_DURATION,
                MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ALL_STREAMS,
                MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
                MF_SOURCE_READER_MEDIASOURCE, MF_VERSION,
            },
            System::{
                Com::StructuredStorage::PROPVARIANT,
                Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
            },
        },
    };

    const HUNDRED_NANOSECONDS_PER_SECOND: i64 = 10_000_000;
    const MAXIMUM_TAIL_SAMPLES: usize = 600;

    pub(super) fn decode_last_frame(video_bytes: &[u8]) -> Result<BgraFrame, String> {
        let mut temporary = tempfile::Builder::new()
            .prefix("chatos-story-tail-")
            .suffix(".mp4")
            .tempfile()
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        temporary
            .write_all(video_bytes)
            .and_then(|_| temporary.flush())
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        let temporary_path = temporary.into_temp_path();
        decode_file(&temporary_path)
    }

    fn decode_file(path: &Path) -> Result<BgraFrame, String> {
        let initialization = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let should_uninitialize = if initialization.is_ok() {
            true
        } else if initialization == RPC_E_CHANGED_MODE {
            false
        } else {
            return Err(VIDEO_TAIL_ERROR.into());
        };
        let _com = ComGuard(should_uninitialize);
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        let _media_foundation = MediaFoundationGuard;
        decode_started(path)
    }

    fn decode_started(path: &Path) -> Result<BgraFrame, String> {
        let mut attributes = None;
        unsafe { MFCreateAttributes(&mut attributes, 1) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        let attributes = attributes.ok_or_else(|| VIDEO_TAIL_ERROR.to_owned())?;
        unsafe { attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;

        let wide = OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let reader = unsafe { MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), &attributes) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        unsafe { reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        unsafe { reader.SetStreamSelection(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, true) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        let native_rotation =
            unsafe { reader.GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, 0) }
                .ok()
                .and_then(|media_type| unsafe { media_type.GetUINT32(&MF_MT_VIDEO_ROTATION) }.ok())
                .unwrap_or(0);

        let media_type = unsafe { MFCreateMediaType() }.map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        unsafe { media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        unsafe { media_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        unsafe {
            reader.SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &media_type,
            )
        }
        .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;

        let duration_value = unsafe {
            reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
        }
        .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        let duration = i64::try_from(&duration_value).map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        if duration <= 0 {
            return Err(VIDEO_TAIL_ERROR.into());
        }
        let requested = duration.saturating_sub(HUNDRED_NANOSECONDS_PER_SECOND);
        let position = PROPVARIANT::from(requested);
        unsafe { reader.SetCurrentPosition(&GUID::zeroed(), &position) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;

        let mut last_sample: Option<IMFSample> = None;
        for _ in 0..MAXIMUM_TAIL_SAMPLES {
            let mut flags = 0_u32;
            let mut sample = None;
            unsafe {
                reader.ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    None,
                    Some(&mut flags),
                    None,
                    Some(&mut sample),
                )
            }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
            if let Some(sample) = sample {
                last_sample = Some(sample);
            }
            if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
                break;
            }
        }
        let sample = last_sample.ok_or_else(|| VIDEO_TAIL_ERROR.to_owned())?;
        let output_type =
            unsafe { reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32) }
                .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        let frame_size = unsafe { output_type.GetUINT64(&MF_MT_FRAME_SIZE) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        let width = (frame_size >> 32) as u32;
        let height = frame_size as u32;
        let stride = unsafe { output_type.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
            .map(|value| value as i32)
            .unwrap_or_else(|_| width.saturating_mul(4) as i32);
        let rotation =
            unsafe { output_type.GetUINT32(&MF_MT_VIDEO_ROTATION) }.unwrap_or(native_rotation);
        let buffer = unsafe { sample.ConvertToContiguousBuffer() }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        let bytes = locked_bytes(&buffer)?;
        Ok(BgraFrame {
            width,
            height,
            stride,
            rotation,
            bytes,
        })
    }

    fn locked_bytes(buffer: &IMFMediaBuffer) -> Result<Vec<u8>, String> {
        let mut pointer = std::ptr::null_mut();
        let mut length = 0_u32;
        unsafe { buffer.Lock(&mut pointer, None, Some(&mut length)) }
            .map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        let copied = if pointer.is_null() || length == 0 {
            Err(VIDEO_TAIL_ERROR.to_owned())
        } else {
            let bytes = unsafe { std::slice::from_raw_parts(pointer, length as usize) };
            Ok(bytes.to_vec())
        };
        unsafe { buffer.Unlock() }.map_err(|_| VIDEO_TAIL_ERROR.to_owned())?;
        copied
    }

    struct ComGuard(bool);

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    struct MediaFoundationGuard;

    impl Drop for MediaFoundationGuard {
        fn drop(&mut self) {
            let _ = unsafe { MFShutdown() };
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::{BgraFrame, VIDEO_TAIL_ERROR};

    pub(super) fn decode_last_frame(_video_bytes: &[u8]) -> Result<BgraFrame, String> {
        Err(VIDEO_TAIL_ERROR.into())
    }
}

#[cfg(test)]
mod tests {
    use super::{encode_png, normalized_rgba, BgraFrame};

    #[cfg(target_os = "windows")]
    use super::last_frame_png;
    #[cfg(target_os = "windows")]
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    fn two_by_two(rotation: u32, stride: i32) -> BgraFrame {
        let top = [[0, 0, 255, 0], [0, 255, 0, 0]];
        let bottom = [[255, 0, 0, 0], [255, 255, 255, 0]];
        let rows = if stride < 0 {
            [bottom, top]
        } else {
            [top, bottom]
        };
        BgraFrame {
            width: 2,
            height: 2,
            stride,
            rotation,
            bytes: rows.into_iter().flatten().flatten().collect(),
        }
    }

    #[test]
    fn normalizes_bgra_and_bottom_up_stride() {
        let (width, height, rgba) = normalized_rgba(&two_by_two(0, -8)).unwrap();
        assert_eq!((width, height), (2, 2));
        assert_eq!(
            rgba,
            vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,]
        );
    }

    #[test]
    fn applies_video_rotation_before_png_encoding() {
        let (width, height, rgba) = normalized_rgba(&two_by_two(90, 8)).unwrap();
        assert_eq!((width, height), (2, 2));
        assert_eq!(&rgba[0..4], &[0, 0, 255, 255]);
        assert_eq!(&rgba[4..8], &[255, 0, 0, 255]);
        assert!(!encode_png(two_by_two(270, 8)).unwrap().is_empty());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn media_foundation_decodes_the_actual_last_h264_frame() {
        let encoded = include_str!("fixtures/tail-red-blue-h264.mp4.b64");
        let video = STANDARD
            .decode(encoded.trim())
            .expect("embedded H.264 MP4 fixture");

        let png = last_frame_png(&video).expect("Media Foundation tail frame");
        let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .expect("decoded tail PNG")
            .to_rgba8();

        assert_eq!(decoded.dimensions(), (64, 64));
        let center = decoded.get_pixel(32, 32).0;
        assert!(
            center[2] > 150 && center[2] > center[0].saturating_add(100),
            "expected the blue final frame, got rgba={center:?}"
        );
    }
}
