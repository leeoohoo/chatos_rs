// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn bundled_release_signature(
    plugin_id: &str,
    version: &str,
    manifest_sha256: String,
    artifact_sha256: &str,
    signed_at: &str,
) -> Result<PluginReleaseSignature, String> {
    let keypair = bundled_signing_keypair()?;
    let mut signature = PluginReleaseSignature {
        key_id: BUNDLED_KEY_ID.to_string(),
        publisher_id: BUNDLED_PUBLISHER_ID.to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        marketplace_id: BUNDLED_MARKETPLACE_ID.to_string(),
        signature_base64: String::new(),
        signed_at: signed_at.to_string(),
        manifest_sha256,
    };
    let payload = plugin_release_signing_payload(
        PluginReleaseVerificationContext {
            plugin_id,
            version,
            marketplace_id: BUNDLED_MARKETPLACE_ID,
            publisher_id: BUNDLED_PUBLISHER_ID,
            artifact_sha256,
        },
        &signature,
    )
    .map_err(|err| err.to_string())?;
    signature.signature_base64 = STANDARD.encode(keypair.sign(payload.as_slice()).as_ref());
    Ok(signature)
}

pub(super) fn bundled_signing_key() -> Result<SigningKeyRef, String> {
    let keypair = bundled_signing_keypair()?;
    Ok(SigningKeyRef {
        key_id: BUNDLED_KEY_ID.to_string(),
        publisher_id: BUNDLED_PUBLISHER_ID.to_string(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        public_key_base64: STANDARD.encode(keypair.public_key().as_ref()),
        usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
        valid_from: BUNDLED_RELEASE_EPOCH.to_string(),
        valid_until: None,
        revoked_at: None,
    })
}

pub(super) fn bundled_signing_keypair() -> Result<Ed25519KeyPair, String> {
    // This deterministic key only attests compile-time bundled content. Network artifacts are
    // never allowed to inherit the bundled marketplace trust scope.
    let seed = Sha256::digest(BUNDLED_SIGNING_SEED_CONTEXT);
    Ed25519KeyPair::from_seed_unchecked(seed.as_slice())
        .map_err(|_| "construct deterministic bundled Plugin attestation key failed".to_string())
}

pub(super) fn bundled_plugin_id(name: &str) -> String {
    format!("bundled-plugin-{name}")
}

pub(super) fn bundled_release_id(name: &str, version: &str) -> String {
    let version = version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("bundled-release-{name}-{version}")
}
