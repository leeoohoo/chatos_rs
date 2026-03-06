use std::path::Path;

use serde_json::{json, Value};

pub fn list_roots() -> Vec<Value> {
    if cfg!(windows) {
        let mut roots = Vec::new();
        for c in b'A'..=b'Z' {
            let drive = format!("{}:\\", c as char);
            if Path::new(&drive).exists() {
                roots.push(json!({
                    "name": drive.clone(),
                    "path": drive,
                    "is_dir": true
                }));
            }
        }
        return roots;
    }
    let mut roots = Vec::new();
    roots.push(json!({
        "name": "/",
        "path": "/",
        "is_dir": true
    }));
    if let Some(home) = home_dir() {
        roots.push(json!({
            "name": home.clone(),
            "path": home,
            "is_dir": true
        }));
    }
    roots
}

fn home_dir() -> Option<String> {
    if let Ok(value) = std::env::var("HOME") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    if let Ok(value) = std::env::var("USERPROFILE") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    let drive = std::env::var("HOMEDRIVE").ok();
    let path = std::env::var("HOMEPATH").ok();
    if let (Some(d), Some(p)) = (drive, path) {
        let d = d.trim().to_string();
        let p = p.trim().to_string();
        if !d.is_empty() || !p.is_empty() {
            return Some(format!("{}{}", d, p));
        }
    }
    None
}
