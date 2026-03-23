use serde_json::{json, Value};
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::SystemTime;
use walkdir::WalkDir;
use zip::write::FileOptions;

pub(super) fn read_dir_entries(path: &Path, include_files: bool) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let iter = fs::read_dir(path).map_err(|e| e.to_string())?;
    for entry in iter {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_dir = meta.is_dir();
        if !is_dir && !include_files {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let p = entry.path().to_string_lossy().to_string();
        let size = if is_dir { None } else { Some(meta.len()) };
        let modified_at = meta.modified().ok().and_then(format_system_time);
        out.push(json!({
            "name": name,
            "path": p,
            "is_dir": is_dir,
            "size": size,
            "modified_at": modified_at
        }));
    }
    out.sort_by(|a, b| {
        let ad = a.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
        let bd = b.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
        if ad != bd {
            return bd.cmp(&ad);
        }
        let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
        an.to_lowercase().cmp(&bn.to_lowercase())
    });
    Ok(out)
}

pub(super) fn is_valid_entry_name(name: &str) -> bool {
    !(name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0'))
}

pub(super) fn infer_download_name(path: &Path) -> String {
    path.file_name()
        .and_then(|v| v.to_str())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "download".to_string())
}

pub(super) fn zip_directory(path: &Path) -> Result<Vec<u8>, String> {
    let root_name = infer_download_name(path);
    let writer = Cursor::new(Vec::<u8>::new());
    let mut zip = zip::ZipWriter::new(writer);
    let dir_options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    let file_options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let root_dir = format!("{}/", path_to_zip_name(Path::new(&root_name)));
    zip.add_directory(root_dir.clone(), dir_options)
        .map_err(|err| err.to_string())?;

    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|err| err.to_string())?;
        let current = entry.path();
        if current == path {
            continue;
        }
        let relative = current.strip_prefix(path).map_err(|err| err.to_string())?;
        let relative_zip_path = path_to_zip_name(relative);
        if relative_zip_path.is_empty() {
            continue;
        }
        let zip_path = format!("{root_name}/{relative_zip_path}");
        if entry.file_type().is_dir() {
            zip.add_directory(format!("{zip_path}/"), dir_options)
                .map_err(|err| err.to_string())?;
            continue;
        }
        if entry.file_type().is_file() {
            zip.start_file(zip_path, file_options)
                .map_err(|err| err.to_string())?;
            let mut file = fs::File::open(current).map_err(|err| err.to_string())?;
            std::io::copy(&mut file, &mut zip).map_err(|err| err.to_string())?;
        }
    }

    let writer = zip.finish().map_err(|err| err.to_string())?;
    Ok(writer.into_inner())
}

fn path_to_zip_name(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn format_system_time(time: SystemTime) -> Option<String> {
    let dt: chrono::DateTime<chrono::Utc> = time.into();
    Some(dt.to_rfc3339())
}
