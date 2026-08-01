// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn native_host_manifest_identity_is_exact() {
    let manifest = ChromeNativeHostManifest {
        name: CHROME_NATIVE_HOST_NAME.to_string(),
        description: CHROME_HOST_DESCRIPTION.to_string(),
        path: "/Applications/ChatOS/chatos_chrome_native_host".to_string(),
        transport_type: "stdio".to_string(),
        allowed_origins: vec![CHROME_EXTENSION_ORIGIN.to_string()],
    };
    assert_eq!(manifest.allowed_origins, [CHROME_EXTENSION_ORIGIN]);
    assert_eq!(manifest.transport_type, "stdio");
}

#[test]
fn native_host_paths_are_platform_specific_and_user_scoped() {
    let home = Path::new("/Users/example");
    let macos = chrome_native_host_manifest_paths_for(ChromeHostPlatform::Macos, home);
    let linux = chrome_native_host_manifest_paths_for(ChromeHostPlatform::Linux, home);
    let windows = chrome_native_host_manifest_paths_for(ChromeHostPlatform::Windows, home);
    assert_eq!(macos.len(), 1);
    assert!(macos[0].ends_with(Path::new(
        "Library/Application Support/Google/Chrome/NativeMessagingHosts/com.chatos.chrome.json"
    )));
    assert_eq!(linux.len(), 2);
    assert!(linux[0].ends_with(Path::new(
        ".config/google-chrome/NativeMessagingHosts/com.chatos.chrome.json"
    )));
    assert!(linux[1].ends_with(Path::new(
        ".config/chromium/NativeMessagingHosts/com.chatos.chrome.json"
    )));
    assert!(
        linux_snap_chromium_manifest_path_for(home).ends_with(Path::new(
            "snap/chromium/common/chromium/NativeMessagingHosts/com.chatos.chrome.json"
        ))
    );
    assert!(linux_snap_chromium_host_path_for(home).ends_with(Path::new(
        "snap/chromium/common/chromium/NativeMessagingHosts/chatos_chrome_native_host"
    )));
    assert!(
        linux_snap_chromium_rendezvous_path_for(home).ends_with(Path::new(
            "snap/chromium/current/.chatos/local_connector/chrome-native-host.json"
        ))
    );
    assert_eq!(windows.len(), 1);
    assert!(windows[0].ends_with(Path::new(
        ".chatos/local_connector/chrome-native-messaging/com.chatos.chrome.json"
    )));
    assert_eq!(
        chrome_native_host_file_name(ChromeHostPlatform::Windows),
        "chatos_chrome_native_host.exe"
    );
    assert_eq!(
        chrome_native_host_manifest_paths_for(ChromeHostPlatform::Unsupported, home),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn windows_registry_binding_is_exact_but_case_and_separator_insensitive() {
    assert_eq!(
        CHROME_WINDOWS_REGISTRY_SUBKEY,
        r"Software\Google\Chrome\NativeMessagingHosts\com.chatos.chrome"
    );
    assert!(windows_registration_paths_match(
        Path::new(r"C:\Users\Example\ChatOS\com.chatos.chrome.json"),
        Path::new("c:/users/example/chatos/com.chatos.chrome.json"),
    )
    .expect("matching Windows registration paths"));
    assert!(!windows_registration_paths_match(
        Path::new(r"C:\Users\Example\ChatOS\com.chatos.chrome.json"),
        Path::new(r"C:\Other\com.chatos.chrome.json"),
    )
    .expect("different Windows registration paths"));
    assert!(normalized_windows_registration_path(Path::new("bad\npath")).is_err());
}

#[test]
fn rendezvous_accepts_only_explicit_loopback_http_origins() {
    assert!(validate_loopback_api_base("http://127.0.0.1:39232/").is_ok());
    assert!(validate_loopback_api_base("https://127.0.0.1:39232/").is_err());
    assert!(validate_loopback_api_base("http://example.com:39232/").is_err());
    assert!(validate_loopback_api_base("http://user:secret@127.0.0.1:39232/").is_err());
}
