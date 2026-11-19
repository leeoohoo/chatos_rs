use super::{InteractiveSshLaunch, RemoteTerminalIdentity};
#[cfg(test)]
pub(super) use crate::interactive_pty::{
    snapshot_state, trim_events, PtySessionState as RemoteTerminalSessionState,
};
use crate::interactive_pty::{ManagedPtySession, PtyCommand, PtyMessages};
pub(super) use crate::interactive_pty::{
    PtyEvent as RemoteTerminalEvent, PtyPollSnapshot as RemoteTerminalPollSnapshot,
};
use std::sync::Arc;

pub(in crate::remote_connection) struct ManagedRemoteTerminal {
    pub(super) identity: RemoteTerminalIdentity,
    pub(super) initial_working_directory: String,
    session: Arc<ManagedPtySession>,
    _launch: InteractiveSshLaunch,
}

impl ManagedRemoteTerminal {
    pub(super) fn spawn(
        identity: RemoteTerminalIdentity,
        launch: InteractiveSshLaunch,
        columns: u16,
        rows: u16,
    ) -> Result<Arc<Self>, String> {
        let initial_working_directory = launch.initial_working_directory.clone();
        let session = ManagedPtySession::spawn(
            PtyCommand {
                executable: launch.executable.clone(),
                arguments: launch.arguments.clone(),
                environment: launch.environment.clone(),
                working_directory: launch.working_directory.clone(),
            },
            columns,
            rows,
            PtyMessages {
                launch_error: "无法启动 Windows OpenSSH 远端终端。",
                isolation_error: "无法隔离远端终端进程树。",
                invalid_input: "远端终端输入无效。",
                io_error: "远端终端通信失败。",
                session_unavailable: "远端终端会话已结束，请重新连接。",
                resize_error: "无法调整远端终端窗口大小。",
                thread_name: "remote",
            },
        )?;
        Ok(Arc::new(Self {
            identity,
            initial_working_directory,
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
    ) -> Result<(RemoteTerminalPollSnapshot, bool), String> {
        self.session.poll(after_cursor, timeout_ms).await
    }
}

impl Drop for ManagedRemoteTerminal {
    fn drop(&mut self) {
        self.close();
    }
}
