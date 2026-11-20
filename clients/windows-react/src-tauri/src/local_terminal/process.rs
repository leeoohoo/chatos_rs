use super::{LocalTerminalIdentity, LocalTerminalLaunch};
#[cfg(test)]
pub(super) use crate::interactive_pty::{
    snapshot_state, trim_events, PtySessionState as LocalTerminalSessionState,
};
use crate::{
    interactive_pty::{ManagedPtySession, PtyCommand, PtyMessages},
    machine_resource_governor::MachineResourcePermit,
};
pub(super) use crate::interactive_pty::{
    PtyEvent as LocalTerminalEvent, PtyPollSnapshot as LocalTerminalPollSnapshot,
};
use std::{ffi::OsString, sync::Arc};

pub(super) struct ManagedLocalTerminal {
    pub(super) identity: LocalTerminalIdentity,
    pub(super) initial_working_directory: String,
    pub(super) shell_name: String,
    session: Arc<ManagedPtySession>,
    _launch: LocalTerminalLaunch,
}

impl ManagedLocalTerminal {
    #[allow(dead_code)]
    pub(super) fn spawn(
        identity: LocalTerminalIdentity,
        launch: LocalTerminalLaunch,
        columns: u16,
        rows: u16,
    ) -> Result<Arc<Self>, String> {
        Self::spawn_with_permit(identity, launch, columns, rows, None)
    }

    pub(super) fn spawn_with_permit(
        identity: LocalTerminalIdentity,
        launch: LocalTerminalLaunch,
        columns: u16,
        rows: u16,
        resource_permit: Option<MachineResourcePermit>,
    ) -> Result<Arc<Self>, String> {
        let initial_working_directory = launch.initial_working_directory.clone();
        let shell_name = launch.shell_name.clone();
        let session = ManagedPtySession::spawn_with_permit(
            PtyCommand {
                executable: launch.executable.clone(),
                arguments: launch
                    .arguments
                    .iter()
                    .map(|argument| OsString::from(argument.as_str()))
                    .collect(),
                environment: launch
                    .environment
                    .iter()
                    .map(|(key, value)| {
                        (OsString::from(key.as_str()), OsString::from(value.as_str()))
                    })
                    .collect(),
                working_directory: launch.working_directory.clone(),
            },
            columns,
            rows,
            PtyMessages {
                launch_error: "无法启动 Windows PowerShell 本机终端。",
                isolation_error: "无法隔离本机终端进程树。",
                invalid_input: "本机终端输入无效。",
                io_error: "本机终端通信失败。",
                session_unavailable: "本机终端会话已结束，请新建终端。",
                resize_error: "无法调整本机终端窗口大小。",
                thread_name: "local",
            },
            resource_permit,
        )?;
        Ok(Arc::new(Self {
            identity,
            initial_working_directory,
            shell_name,
            session,
            _launch: launch,
        }))
    }

    pub(super) fn belongs_to(&self, owner_user_id: &str, presentation_id: &str) -> bool {
        self.identity.owner_user_id == owner_user_id
            && self.identity.presentation_id == presentation_id
    }

    pub(super) fn send(&self, bytes: &[u8]) -> Result<(), String> {
        self.session.send(bytes)
    }

    pub(super) fn resize(&self, columns: u16, rows: u16) -> Result<(), String> {
        self.session.resize(columns, rows)
    }

    pub(super) fn close(&self) {
        self.session.close();
    }

    pub(super) async fn poll(
        &self,
        after_cursor: u64,
        timeout_ms: u64,
    ) -> Result<(LocalTerminalPollSnapshot, bool), String> {
        self.session.poll(after_cursor, timeout_ms).await
    }
}

impl Drop for ManagedLocalTerminal {
    fn drop(&mut self) {
        self.close();
    }
}
