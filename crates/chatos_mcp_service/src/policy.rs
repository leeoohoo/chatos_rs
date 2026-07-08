// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub const BUILTIN_KIND_CODE_MAINTAINER_READ: &str = "CodeMaintainerRead";
pub const BUILTIN_KIND_CODE_MAINTAINER_WRITE: &str = "CodeMaintainerWrite";
pub const BUILTIN_KIND_TERMINAL_CONTROLLER: &str = "TerminalController";
pub const BUILTIN_KIND_BROWSER_TOOLS: &str = "BrowserTools";

pub const LOCAL_CONNECTOR_ENABLED_BUILTIN_KINDS_HEADER: &str =
    "x-local-connector-enabled-builtin-kinds";
pub const HARNESS_CODE_ENABLED_BUILTIN_KINDS_HEADER: &str = "x-harness-code-enabled-builtin-kinds";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinHostBackend {
    LocalConnector,
    HarnessCode,
}

impl BuiltinHostBackend {
    pub fn replaces_builtin_kind_name(self, value: &str) -> bool {
        match self {
            Self::LocalConnector => matches!(
                normalize_builtin_kind_name(value),
                Some(
                    BUILTIN_KIND_CODE_MAINTAINER_READ
                        | BUILTIN_KIND_CODE_MAINTAINER_WRITE
                        | BUILTIN_KIND_TERMINAL_CONTROLLER
                        | BUILTIN_KIND_BROWSER_TOOLS
                )
            ),
            Self::HarnessCode => matches!(
                normalize_builtin_kind_name(value),
                Some(BUILTIN_KIND_CODE_MAINTAINER_READ | BUILTIN_KIND_CODE_MAINTAINER_WRITE)
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinToolAccess {
    CodeRead,
    CodeWrite,
    Terminal,
    Browser,
}

impl BuiltinToolAccess {
    pub fn required_builtin_kind_name(self) -> &'static str {
        match self {
            Self::CodeRead => BUILTIN_KIND_CODE_MAINTAINER_READ,
            Self::CodeWrite => BUILTIN_KIND_CODE_MAINTAINER_WRITE,
            Self::Terminal => BUILTIN_KIND_TERMINAL_CONTROLLER,
            Self::Browser => BUILTIN_KIND_BROWSER_TOOLS,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HostCapabilityPolicy {
    pub code_read: bool,
    pub code_write: bool,
    pub terminal: bool,
    pub browser: bool,
}

impl HostCapabilityPolicy {
    pub fn from_header_value(raw: &str) -> Self {
        Self::from_builtin_kind_names(split_builtin_kind_header(raw))
    }

    pub fn from_builtin_kind_names<I, S>(values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut policy = Self::default();
        for value in values {
            policy.enable_builtin_kind_name(value.as_ref());
        }
        policy
    }

    pub fn enable_builtin_kind_name(&mut self, value: &str) {
        match normalize_builtin_kind_name(value) {
            Some(BUILTIN_KIND_CODE_MAINTAINER_READ) => self.code_read = true,
            Some(BUILTIN_KIND_CODE_MAINTAINER_WRITE) => {
                self.code_read = true;
                self.code_write = true;
            }
            Some(BUILTIN_KIND_TERMINAL_CONTROLLER) => self.terminal = true,
            Some(BUILTIN_KIND_BROWSER_TOOLS) => self.browser = true,
            _ => {}
        }
    }

    pub fn enables_builtin_kind_name(&self, value: &str) -> bool {
        match normalize_builtin_kind_name(value) {
            Some(BUILTIN_KIND_CODE_MAINTAINER_READ) => self.code_read,
            Some(BUILTIN_KIND_CODE_MAINTAINER_WRITE) => self.code_write,
            Some(BUILTIN_KIND_TERMINAL_CONTROLLER) => self.terminal,
            Some(BUILTIN_KIND_BROWSER_TOOLS) => self.browser,
            _ => false,
        }
    }

    pub fn allows_tool(&self, name: &str) -> bool {
        match classify_builtin_tool(name) {
            Some(BuiltinToolAccess::CodeRead) => self.code_read,
            Some(BuiltinToolAccess::CodeWrite) => self.code_write,
            Some(BuiltinToolAccess::Terminal) => self.terminal,
            Some(BuiltinToolAccess::Browser) => self.browser,
            None => false,
        }
    }

    pub fn enabled_builtin_kind_names(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.code_read {
            out.push(BUILTIN_KIND_CODE_MAINTAINER_READ);
        }
        if self.code_write {
            out.push(BUILTIN_KIND_CODE_MAINTAINER_WRITE);
        }
        if self.terminal {
            out.push(BUILTIN_KIND_TERMINAL_CONTROLLER);
        }
        if self.browser {
            out.push(BUILTIN_KIND_BROWSER_TOOLS);
        }
        out
    }
}

pub fn selected_host_builtin_kind_names<I, S>(
    host: BuiltinHostBackend,
    values: I,
) -> Vec<&'static str>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    HostCapabilityPolicy::from_builtin_kind_names(
        values
            .into_iter()
            .filter(|value| host.replaces_builtin_kind_name(value.as_ref())),
    )
    .enabled_builtin_kind_names()
    .into_iter()
    .filter(|value| host.replaces_builtin_kind_name(value))
    .collect()
}

pub fn builtin_kind_header_value<I, S>(values: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    values
        .into_iter()
        .filter_map(|value| normalize_builtin_kind_name(value.as_ref()))
        .fold(Vec::new(), |mut out, value| {
            if !out.contains(&value) {
                out.push(value);
            }
            out
        })
        .join(",")
}

pub fn split_builtin_kind_header(raw: &str) -> impl Iterator<Item = &str> {
    raw.split([',', ';', '|', ' '])
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn normalize_builtin_kind_name(value: &str) -> Option<&'static str> {
    match normalize_token(value).as_str() {
        "codemaintainerread" => Some(BUILTIN_KIND_CODE_MAINTAINER_READ),
        "codemaintainerwrite" => Some(BUILTIN_KIND_CODE_MAINTAINER_WRITE),
        "terminalcontroller" => Some(BUILTIN_KIND_TERMINAL_CONTROLLER),
        "browsertools" => Some(BUILTIN_KIND_BROWSER_TOOLS),
        _ => None,
    }
}

pub fn classify_builtin_tool(name: &str) -> Option<BuiltinToolAccess> {
    match name.trim() {
        "read_file_raw" | "read_file_range" | "read_file" | "list_dir" | "search_text"
        | "search_files" => Some(BuiltinToolAccess::CodeRead),
        "write_file" | "edit_file" | "append_file" | "delete_path" | "apply_patch" | "patch" => {
            Some(BuiltinToolAccess::CodeWrite)
        }
        "execute_command" | "get_recent_logs" | "process_list" | "process_poll" | "process_log"
        | "process_wait" | "process_write" | "process_kill" | "process" => {
            Some(BuiltinToolAccess::Terminal)
        }
        "browser_navigate" | "browser_snapshot" | "browser_click" | "browser_type"
        | "browser_scroll" | "browser_back" | "browser_press" | "browser_console"
        | "browser_get_images" | "browser_inspect" | "browser_research" | "browser_vision" => {
            Some(BuiltinToolAccess::Browser)
        }
        _ => None,
    }
}

fn normalize_token(token: &str) -> String {
    token
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_capability_implies_read() {
        let policy = HostCapabilityPolicy::from_header_value("CodeMaintainerWrite");

        assert!(policy.code_read);
        assert!(policy.code_write);
        assert!(policy.allows_tool("read_file_raw"));
        assert!(policy.allows_tool("apply_patch"));
    }

    #[test]
    fn host_backend_filters_supported_kinds() {
        assert_eq!(
            selected_host_builtin_kind_names(
                BuiltinHostBackend::HarnessCode,
                [
                    "TerminalController",
                    "CodeMaintainerWrite",
                    "BrowserTools",
                    "CodeMaintainerRead",
                ],
            ),
            vec![
                BUILTIN_KIND_CODE_MAINTAINER_READ,
                BUILTIN_KIND_CODE_MAINTAINER_WRITE,
            ]
        );
        assert!(BuiltinHostBackend::LocalConnector.replaces_builtin_kind_name("browser_tools"));
    }

    #[test]
    fn classifies_builtin_tool_access() {
        assert_eq!(
            classify_builtin_tool("search_text"),
            Some(BuiltinToolAccess::CodeRead)
        );
        assert_eq!(
            classify_builtin_tool("delete_path"),
            Some(BuiltinToolAccess::CodeWrite)
        );
        assert_eq!(
            classify_builtin_tool("process_wait"),
            Some(BuiltinToolAccess::Terminal)
        );
        assert_eq!(
            classify_builtin_tool("browser_inspect"),
            Some(BuiltinToolAccess::Browser)
        );
        assert_eq!(classify_builtin_tool("local_fs_read"), None);
    }

    #[test]
    fn header_value_normalizes_and_dedupes() {
        assert_eq!(
            builtin_kind_header_value([
                " CodeMaintainerWrite ",
                "CodeMaintainerRead",
                "CodeMaintainerWrite",
                "unknown",
            ]),
            "CodeMaintainerWrite,CodeMaintainerRead"
        );
    }
}
