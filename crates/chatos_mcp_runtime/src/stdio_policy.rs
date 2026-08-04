// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

const MAX_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
const MAX_ARGUMENTS_BYTES: usize = 128 * 1024;
const MAX_ENVIRONMENT_VARIABLES: usize = 128;
const MAX_ENVIRONMENT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdioPolicyViolation {
    Arguments,
    EnvironmentLimits,
    EnvironmentEntry,
}

pub fn validate_stdio_arguments(args: &[String]) -> Result<(), StdioPolicyViolation> {
    if args.len() > MAX_ARGUMENTS
        || args
            .iter()
            .any(|arg| arg.len() > MAX_ARGUMENT_BYTES || arg.contains('\0'))
        || args.iter().map(String::len).sum::<usize>() > MAX_ARGUMENTS_BYTES
    {
        return Err(StdioPolicyViolation::Arguments);
    }
    Ok(())
}

pub fn validate_stdio_environment(
    env: &BTreeMap<String, String>,
) -> Result<(), StdioPolicyViolation> {
    if env.len() > MAX_ENVIRONMENT_VARIABLES
        || env
            .iter()
            .map(|(name, value)| name.len().saturating_add(value.len()))
            .sum::<usize>()
            > MAX_ENVIRONMENT_BYTES
    {
        return Err(StdioPolicyViolation::EnvironmentLimits);
    }
    for (name, value) in env {
        validate_stdio_environment_name(name)?;
        if value.contains('\0') {
            return Err(StdioPolicyViolation::EnvironmentEntry);
        }
    }
    Ok(())
}

pub fn validate_stdio_environment_name(name: &str) -> Result<(), StdioPolicyViolation> {
    let valid = !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    let normalized = name.to_ascii_uppercase();
    let controlled = matches!(
        normalized.as_str(),
        "PATH"
            | "HOME"
            | "SHELL"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "COMSPEC"
            | "PATHEXT"
            | "SYSTEMROOT"
            | "WINDIR"
            | "USERPROFILE"
            | "APPDATA"
            | "LOCALAPPDATA"
            | "CHATOS_WORKSPACE"
            | "CHATOS_SANDBOX_MCP_TOKEN"
            | "CHATOS_AGENT_TOKEN"
            | "CHATOS_CLOUD_STDIO_LAUNCH_SPEC_PATH"
            | "NODE_OPTIONS"
            | "PYTHONHOME"
            | "PYTHONPATH"
            | "RUBYOPT"
            | "PERL5OPT"
            | "BASH_ENV"
            | "ENV"
            | "PROMPT_COMMAND"
    ) || normalized.starts_with("LD_")
        || normalized.starts_with("DYLD_")
        || normalized.starts_with("XDG_")
        || normalized.starts_with("MCP_MANAGEMENT_")
        || normalized.starts_with("SANDBOX_MANAGER_");
    if !valid || controlled {
        return Err(StdioPolicyViolation::EnvironmentEntry);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_host_environment_is_rejected_consistently() {
        for name in [
            "PATH",
            "CHATOS_CLOUD_STDIO_LAUNCH_SPEC_PATH",
            "LD_PRELOAD",
            "MCP_MANAGEMENT_SECRET",
        ] {
            assert_eq!(
                validate_stdio_environment_name(name),
                Err(StdioPolicyViolation::EnvironmentEntry)
            );
        }
        assert!(validate_stdio_environment_name("PLUGIN_ACCESS_TOKEN").is_ok());
    }

    #[test]
    fn argument_and_environment_limits_reject_nul_and_oversized_inputs() {
        assert_eq!(
            validate_stdio_arguments(&["bad\0argument".to_string()]),
            Err(StdioPolicyViolation::Arguments)
        );
        assert_eq!(
            validate_stdio_environment(&BTreeMap::from([(
                "PLUGIN_TOKEN".to_string(),
                "bad\0value".to_string(),
            )])),
            Err(StdioPolicyViolation::EnvironmentEntry)
        );
    }
}
