use super::{plugin_catalog_error, resolve_installed_file};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::http;

const MAXIMUM_ICON_SOURCE_BYTES: u64 = 5 * 1_024 * 1_024;
const MAXIMUM_ICON_SOURCE_PIXELS: u64 = 16_000_000;
const MAXIMUM_ICON_DISPLAY_PIXELS: u32 = 256;
pub(super) const MAXIMUM_ICON_PNG_BYTES: usize = 512 * 1_024;
const MAXIMUM_ICON_SESSIONS: usize = 8;

#[derive(Clone, Default)]
pub(crate) struct PluginApplicationIconStore {
    active: Arc<Mutex<VecDeque<PluginApplicationIconSession>>>,
    generation: Arc<AtomicU64>,
}

struct PluginApplicationIconSession {
    token: String,
    icons: BTreeMap<String, PluginApplicationIconSource>,
}

#[derive(Clone)]
pub(super) struct PluginApplicationIconSource {
    pub(super) plugin_root: PathBuf,
    pub(super) installation_root: PathBuf,
    pub(super) relative: String,
}

impl PluginApplicationIconStore {
    pub(super) fn publish(
        &self,
        expected_generation: u64,
        token: String,
        icons: BTreeMap<String, PluginApplicationIconSource>,
    ) -> Result<(), String> {
        if uuid::Uuid::parse_str(&token).is_err()
            || icons.keys().any(|key| uuid::Uuid::parse_str(key).is_err())
        {
            return Err(plugin_catalog_error());
        }
        if self.generation.load(Ordering::Acquire) != expected_generation {
            return Ok(());
        }
        let mut active = self.active.lock().map_err(|_| plugin_catalog_error())?;
        if self.generation.load(Ordering::Acquire) != expected_generation {
            return Ok(());
        }
        active.push_back(PluginApplicationIconSession { token, icons });
        while active.len() > MAXIMUM_ICON_SESSIONS {
            active.pop_front();
        }
        Ok(())
    }

    pub(super) fn reset(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut active) = self.active.lock() {
            active.clear();
        }
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub(super) fn protocol_response(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> http::Response<Vec<u8>> {
        if request.method() != http::Method::GET {
            return http::Response::builder()
                .status(http::StatusCode::METHOD_NOT_ALLOWED)
                .header(http::header::ALLOW, "GET")
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(b"Plugin application icon unavailable".to_vec())
                .expect("valid Plugin application icon method response");
        }
        match self.protocol_body(request.uri().path()) {
            Ok(body) => http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "image/png")
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-store")
                .body(body)
                .expect("valid Plugin application icon response"),
            Err(status) => http::Response::builder()
                .status(status)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(b"Plugin application icon unavailable".to_vec())
                .expect("valid Plugin application icon error response"),
        }
    }

    pub(super) fn protocol_body(&self, path: &str) -> Result<Vec<u8>, http::StatusCode> {
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() != 2
            || uuid::Uuid::parse_str(parts[0]).is_err()
            || uuid::Uuid::parse_str(parts[1]).is_err()
        {
            return Err(http::StatusCode::BAD_REQUEST);
        }
        let active = self
            .active
            .lock()
            .map_err(|_| http::StatusCode::SERVICE_UNAVAILABLE)?;
        let source = active
            .iter()
            .rev()
            .find(|session| session.token == parts[0])
            .ok_or(http::StatusCode::UNAUTHORIZED)?
            .icons
            .get(parts[1])
            .cloned()
            .ok_or(http::StatusCode::NOT_FOUND)?;
        drop(active);
        normalize_plugin_application_icon(
            &source.plugin_root,
            &source.installation_root,
            &source.relative,
        )
        .map_err(|_| http::StatusCode::UNSUPPORTED_MEDIA_TYPE)
    }
}

pub(crate) fn plugin_application_icon_protocol(
    icons: PluginApplicationIconStore,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    icons.protocol_response(request)
}

fn normalize_plugin_application_icon(
    plugin_root: &Path,
    installation_root: &Path,
    relative: &str,
) -> Result<Vec<u8>, String> {
    let source = resolve_installed_file(plugin_root, installation_root, relative)?;
    let metadata = source.metadata().map_err(|_| plugin_catalog_error())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAXIMUM_ICON_SOURCE_BYTES {
        return Err(plugin_catalog_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(&source)
        .and_then(|file| {
            file.take(MAXIMUM_ICON_SOURCE_BYTES + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| plugin_catalog_error())?;
    if bytes.is_empty() || bytes.len() as u64 > MAXIMUM_ICON_SOURCE_BYTES {
        return Err(plugin_catalog_error());
    }
    if source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        return normalize_plugin_application_svg(&bytes);
    }
    let format = image::guess_format(&bytes).map_err(|_| plugin_catalog_error())?;
    if !matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::Gif
            | image::ImageFormat::WebP
    ) {
        return Err(plugin_catalog_error());
    }
    let (width, height) = image::ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| plugin_catalog_error())?;
    if width == 0
        || height == 0
        || u64::from(width)
            .checked_mul(u64::from(height))
            .filter(|pixels| *pixels <= MAXIMUM_ICON_SOURCE_PIXELS)
            .is_none()
    {
        return Err(plugin_catalog_error());
    }
    let decoded =
        image::load_from_memory_with_format(&bytes, format).map_err(|_| plugin_catalog_error())?;
    let normalized = decoded.thumbnail(MAXIMUM_ICON_DISPLAY_PIXELS, MAXIMUM_ICON_DISPLAY_PIXELS);
    let mut output = Cursor::new(Vec::new());
    normalized
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| plugin_catalog_error())?;
    let output = output.into_inner();
    if output.is_empty() || output.len() > MAXIMUM_ICON_PNG_BYTES {
        return Err(plugin_catalog_error());
    }
    Ok(output)
}

pub(super) fn normalize_plugin_application_svg(bytes: &[u8]) -> Result<Vec<u8>, String> {
    // Plugin manifests are untrusted. Disable external and nested data image resolvers.
    let options = resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &options).map_err(|_| plugin_catalog_error())?;
    let source_size = tree.size();
    let source_width = source_size.width();
    let source_height = source_size.height();
    if !source_width.is_finite()
        || !source_height.is_finite()
        || source_width <= 0.0
        || source_height <= 0.0
        || f64::from(source_width) * f64::from(source_height) > MAXIMUM_ICON_SOURCE_PIXELS as f64
    {
        return Err(plugin_catalog_error());
    }
    let scale = (MAXIMUM_ICON_DISPLAY_PIXELS as f32 / source_width)
        .min(MAXIMUM_ICON_DISPLAY_PIXELS as f32 / source_height);
    let width = (source_width * scale)
        .ceil()
        .clamp(1.0, MAXIMUM_ICON_DISPLAY_PIXELS as f32) as u32;
    let height = (source_height * scale)
        .ceil()
        .clamp(1.0, MAXIMUM_ICON_DISPLAY_PIXELS as f32) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or_else(plugin_catalog_error)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let output = pixmap.encode_png().map_err(|_| plugin_catalog_error())?;
    if output.is_empty() || output.len() > MAXIMUM_ICON_PNG_BYTES {
        return Err(plugin_catalog_error());
    }
    Ok(output)
}

pub(super) fn plugin_application_icon_url(session_token: &str, icon_token: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("http://chatos-plugin-icon.localhost/{session_token}/{icon_token}")
    } else {
        format!("chatos-plugin-icon://localhost/{session_token}/{icon_token}")
    }
}
