// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use tokio::process::{Child, Command};

pub async fn terminate_child_process_tree(child: &mut Child) -> Result<(), String> {
    if child
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Ok(());
    }

    #[cfg(unix)]
    {
        let process_group_error = child.id().and_then(|pid| {
            let result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
            if result == 0 {
                None
            } else {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::ESRCH) {
                    None
                } else {
                    Some(error.to_string())
                }
            }
        });
        let _ = child.kill().await;
        let _ = child.wait().await;
        if let Some(error) = process_group_error {
            return Err(error);
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        child.kill().await.map_err(|error| error.to_string())?;
        let _ = child.wait().await;
        Ok(())
    }
}

#[cfg(unix)]
pub fn configure_child_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.as_std_mut().process_group(0);
}

#[cfg(not(unix))]
pub fn configure_child_process_group(_command: &mut Command) {}
