use super::*;

impl RunService {
    pub(in crate::services) async fn execute_prepared_model_run(
        &self,
        task: &TaskRecord,
        run: &TaskRunRecord,
        model_config: &ModelConfigRecord,
        prepared_execution: PreparedModelExecution,
    ) -> TaskRunReport {
        let runtime_execution = self.build_runtime_execution_state(
            run,
            model_config,
            &prepared_execution.run_spec,
            prepared_execution.tool_result_model_budget_limits,
        );
        let execution_timeout = match self.effective_execution_timeout().await {
            Ok(timeout) => timeout,
            Err(err) => {
                return TaskRunReport::from_ai_report(
                    task.id.clone(),
                    run.id.clone(),
                    Some(model_config.id.clone()),
                    AiTurnReport::failed(format!("failed to resolve execution timeout: {err}")),
                );
            }
        };
        let execution = TaskRunExecution::new(
            prepared_execution.runtime_config,
            prepared_execution.run_spec,
        );
        let report = match tokio::time::timeout(
            execution_timeout,
            execution.run_report_with_mcp_builder_and_options(
                prepared_execution.mcp_builder,
                runtime_execution.runtime_options,
            ),
        )
        .await
        {
            Ok(report) => report,
            Err(_) => TaskRunReport::from_ai_report(
                task.id.clone(),
                run.id.clone(),
                Some(model_config.id.clone()),
                AiTurnReport::failed(format!(
                    "execution timed out after {} seconds",
                    execution_timeout.as_secs()
                )),
            ),
        };
        runtime_execution
            .stop_cancel_poll
            .store(true, Ordering::Relaxed);
        runtime_execution.cancel_poll_handle.abort();
        flush_pending_stream_event(
            &self.store,
            run.id.as_str(),
            &runtime_execution.pending_stream_event,
        );
        report
    }
}
