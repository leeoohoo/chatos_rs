pub(crate) fn windows_compatible(platforms: &[String]) -> bool {
    platforms.is_empty()
        || platforms.iter().any(|platform| {
            let platform = platform.trim().to_ascii_lowercase();
            platform == "windows" || current_windows_platforms().contains(&platform.as_str())
        })
}

#[cfg(target_arch = "x86_64")]
fn current_windows_platforms() -> &'static [&'static str] {
    &["windows-x64", "windows-x86_64"]
}

#[cfg(target_arch = "aarch64")]
fn current_windows_platforms() -> &'static [&'static str] {
    &["windows-arm64", "windows-aarch64"]
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn current_windows_platforms() -> &'static [&'static str] {
    &[]
}

#[cfg(test)]
mod tests {
    use super::windows_compatible;

    #[test]
    fn accepts_universal_and_current_windows_architecture() {
        assert!(windows_compatible(&[]));
        assert!(windows_compatible(&["windows".into()]));
        #[cfg(target_arch = "x86_64")]
        {
            assert!(windows_compatible(&["windows-x64".into()]));
            assert!(windows_compatible(&["windows-x86_64".into()]));
            assert!(!windows_compatible(&["windows-arm64".into()]));
        }
        #[cfg(target_arch = "aarch64")]
        {
            assert!(windows_compatible(&["windows-arm64".into()]));
            assert!(windows_compatible(&["windows-aarch64".into()]));
            assert!(!windows_compatible(&["windows-x64".into()]));
        }
    }

    #[test]
    fn rejects_other_families_and_unknown_architectures() {
        assert!(!windows_compatible(&["macos-arm64".into(), "linux".into()]));
        assert!(!windows_compatible(&["windows-mips64".into()]));
        assert!(!windows_compatible(&["windows-".into()]));
    }
}
