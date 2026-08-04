// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::env;
use std::fs;
use std::path::Path;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key_path = env::args()
        .nth(1)
        .ok_or("usage: local_connector_dev_relay_keygen <pkcs8-der-path>")?;
    let key_path = Path::new(key_path.as_str());
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let keypair = load_or_generate_keypair(key_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(key_path, fs::Permissions::from_mode(0o600))?;
    }

    println!(
        "ed25519:{}",
        URL_SAFE_NO_PAD.encode(keypair.public_key().as_ref())
    );
    Ok(())
}

fn load_or_generate_keypair(path: &Path) -> Result<Ed25519KeyPair, Box<dyn std::error::Error>> {
    if let Ok(bytes) = fs::read(path) {
        if let Ok(keypair) = Ed25519KeyPair::from_pkcs8(bytes.as_slice()) {
            return Ok(keypair);
        }
    }
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| "generate relay Ed25519 PKCS8 failed")?;
    fs::write(path, pkcs8.as_ref())?;
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| "reload generated relay key failed".into())
}
