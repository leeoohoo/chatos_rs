// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::time::Duration;

use tempfile::TempDir;

use super::*;

fn scope(
    owner_user_id: &str,
    device_id: &str,
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
    secret_name: &str,
) -> PluginCredentialScope {
    PluginCredentialScope::new(
        owner_user_id,
        device_id,
        plugin_id,
        release_id,
        component_key,
        secret_name,
    )
    .expect("valid test credential scope")
}

#[test]
fn handles_are_bound_to_the_exact_plugin_release_component_user_and_device() {
    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let exact = scope(
        "user-a",
        "device-a",
        "plugin-a",
        "release-a",
        "component-a",
        "api-key",
    );
    vault
        .upsert(&exact, b"credential-value")
        .expect("store credential");
    let handle = vault
        .issue_handle(&exact, Duration::from_secs(30))
        .expect("issue handle");
    assert_eq!(
        vault
            .resolve_handle(handle.as_str(), &exact)
            .expect("resolve exact scope")
            .as_bytes(),
        b"credential-value"
    );

    for different in [
        scope(
            "user-b",
            "device-a",
            "plugin-a",
            "release-a",
            "component-a",
            "api-key",
        ),
        scope(
            "user-a",
            "device-b",
            "plugin-a",
            "release-a",
            "component-a",
            "api-key",
        ),
        scope(
            "user-a",
            "device-a",
            "plugin-b",
            "release-a",
            "component-a",
            "api-key",
        ),
        scope(
            "user-a",
            "device-a",
            "plugin-a",
            "release-b",
            "component-a",
            "api-key",
        ),
        scope(
            "user-a",
            "device-a",
            "plugin-a",
            "release-a",
            "component-b",
            "api-key",
        ),
        scope(
            "user-a",
            "device-a",
            "plugin-a",
            "release-a",
            "component-a",
            "other-secret",
        ),
    ] {
        assert!(vault.resolve_handle(handle.as_str(), &different).is_err());
    }
}

#[test]
fn handles_expire_revoke_and_fail_closed_after_secret_changes() {
    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let credential = scope(
        "user-a",
        "device-a",
        "plugin-a",
        "release-a",
        "component-a",
        "token",
    );
    vault.upsert(&credential, b"first").expect("store secret");

    let expiring = vault
        .issue_handle(&credential, Duration::from_millis(5))
        .expect("issue expiring handle");
    std::thread::sleep(Duration::from_millis(15));
    assert!(vault
        .resolve_handle(expiring.as_str(), &credential)
        .is_err());

    let revoked = vault
        .issue_handle(&credential, Duration::from_secs(30))
        .expect("issue revocable handle");
    assert!(vault
        .revoke_handle(revoked.as_str())
        .expect("revoke handle"));
    assert!(vault.resolve_handle(revoked.as_str(), &credential).is_err());

    let stale = vault
        .issue_handle(&credential, Duration::from_secs(30))
        .expect("issue stale handle");
    vault.upsert(&credential, b"second").expect("rotate secret");
    assert!(vault.resolve_handle(stale.as_str(), &credential).is_err());

    let deleted = vault
        .issue_handle(&credential, Duration::from_secs(30))
        .expect("issue deleted handle");
    assert!(vault.delete(&credential).expect("delete secret"));
    assert!(vault.resolve_handle(deleted.as_str(), &credential).is_err());
    assert!(!vault.delete(&credential).expect("idempotent delete"));
}

#[test]
fn metadata_and_plugin_files_never_contain_secret_plaintext() {
    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let credential = scope(
        "user-a",
        "device-a",
        "plugin-a",
        "release-a",
        "component-a",
        "api-key",
    );
    let secret = "never-write-this-plugin-secret";
    vault
        .upsert(&credential, secret.as_bytes())
        .expect("store secret");

    let plugin_root = temp.path().join("plugins");
    fs::create_dir_all(plugin_root.join("installed/plugin-a/1.0.0"))
        .expect("create installation directory");
    fs::write(plugin_root.join("state.json"), b"{\"plugins\":{}}").expect("write registry fixture");
    fs::write(
        plugin_root.join("transactions.json"),
        b"{\"active\":[],\"history\":[]}",
    )
    .expect("write journal fixture");

    let metadata =
        fs::read_to_string(plugin_root.join("credentials.json")).expect("read credential metadata");
    assert!(!metadata.contains(secret));
    assert!(metadata.contains("api-key"));
    assert!(metadata.contains("release-a"));

    for path in [
        plugin_root.join("state.json"),
        plugin_root.join("transactions.json"),
    ] {
        assert!(!fs::read_to_string(path)
            .expect("read Plugin state")
            .contains(secret));
    }
    assert!(!plugin_root
        .join("installed/plugin-a/1.0.0")
        .join(secret)
        .exists());
}

#[test]
fn purge_release_and_plugin_remove_only_matching_scopes() {
    let temp = TempDir::new().expect("temp directory");
    let vault = PluginCredentialVault::in_memory(temp.path());
    let release_one = scope(
        "user-a",
        "device-a",
        "plugin-a",
        "release-1",
        "component-a",
        "token",
    );
    let release_two = scope(
        "user-a",
        "device-a",
        "plugin-a",
        "release-2",
        "component-a",
        "token",
    );
    let other_plugin = scope(
        "user-a",
        "device-a",
        "plugin-b",
        "release-1",
        "component-a",
        "token",
    );
    for credential in [&release_one, &release_two, &other_plugin] {
        vault.upsert(credential, b"value").expect("store secret");
    }
    let release_one_handle = vault
        .issue_handle(&release_one, Duration::from_secs(30))
        .expect("issue release one handle");

    assert_eq!(
        vault
            .purge_release("plugin-a", "release-1")
            .expect("purge release"),
        1
    );
    assert!(vault
        .resolve_handle(release_one_handle.as_str(), &release_one)
        .is_err());
    assert_eq!(
        vault
            .list("user-a", "device-a", "plugin-a", "release-2")
            .expect("list release two")
            .len(),
        1
    );
    assert_eq!(vault.purge_plugin("plugin-a").expect("purge plugin"), 1);
    assert_eq!(
        vault
            .list("user-a", "device-a", "plugin-b", "release-1")
            .expect("list other plugin")
            .len(),
        1
    );
}

#[test]
fn scope_hash_is_unambiguous_and_storage_safe() {
    let left = scope("ab", "c", "p", "r", "component", "secret");
    let right = scope("a", "bc", "p", "r", "component", "secret");
    assert_ne!(left.scope_hash(), right.scope_hash());
    assert!(left
        .scope_hash()
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit()));
}
