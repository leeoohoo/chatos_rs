use super::*;

mod execute;
mod inspect;
mod interaction;

#[async_trait]
impl TerminalControllerStore for TaskRunnerTerminalControllerStore {
    async fn execute_command(
        &self,
        context: TerminalControllerContext,
        path: String,
        command: String,
        background: bool,
    ) -> Result<Value, String> {
        self.execute_command_value(context, path, command, background)
            .await
    }

    async fn get_recent_logs(
        &self,
        context: TerminalControllerContext,
        per_terminal_limit: i64,
        terminal_limit: usize,
    ) -> Result<Value, String> {
        self.get_recent_logs_value(context, per_terminal_limit, terminal_limit)
            .await
    }

    async fn process_list(
        &self,
        context: TerminalControllerContext,
        include_exited: bool,
        limit: usize,
    ) -> Result<Value, String> {
        self.process_list_value(context, include_exited, limit)
            .await
    }

    async fn process_poll(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String> {
        self.process_poll_value(context, terminal_id, offset, limit)
            .await
    }

    async fn process_log(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        offset: Option<i64>,
        limit: i64,
    ) -> Result<Value, String> {
        self.process_log_value(context, terminal_id, offset, limit)
            .await
    }

    async fn process_wait(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        timeout_ms: u64,
    ) -> Result<Value, String> {
        self.process_wait_value(context, terminal_id, timeout_ms)
            .await
    }

    async fn process_write(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
        data: String,
        submit: bool,
    ) -> Result<Value, String> {
        self.process_write_value(context, terminal_id, data, submit)
            .await
    }

    async fn process_kill(
        &self,
        context: TerminalControllerContext,
        terminal_id: String,
    ) -> Result<Value, String> {
        self.process_kill_value(context, terminal_id).await
    }
}
