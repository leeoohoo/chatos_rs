const TASK_OUTCOME_REPORT_TOOL: &str = "task_run_process_report_outcome";
const INLINE_PREREQUISITE_RESULT_MAX_CHARS: usize = 160_000;

// Keep dependency-gating queries as raw SQL. Rust's escaped-newline string syntax
// removes both the newline and indentation; a missing space before `\` can silently
// join SQL tokens (for example `runJOIN`) and is especially risky in these CTEs.

macro_rules! dependency_gating_sql {
    ($update:literal) => {
        concat!(
            r#"
WITH prerequisite_results AS (
  SELECT task.task_id,
         task.status,
         COALESCE(
           (
             SELECT COALESCE(
               json_extract(invocation.result_json, '$.result_status'),
               json_extract(invocation.result_json, '$.status'),
               json_extract(invocation.arguments_json, '$.result_status'),
               json_extract(invocation.arguments_json, '$.status')
             )
             FROM local_agent_runs run
             JOIN local_agent_tool_invocations invocation ON invocation.run_id = run.run_id
             WHERE run.owner_entity_type = 'task'
               AND run.owner_entity_id = task.task_id
               AND run.status = 'succeeded'
               AND invocation.tool_name = 'task_run_process_report_outcome'
               AND invocation.status = 'succeeded'
             ORDER BY run.updated_at_unix_ms DESC,
                      run.created_at_unix_ms DESC,
                      run.rowid DESC,
                      invocation.updated_at_unix_ms DESC,
                      invocation.rowid DESC
             LIMIT 1
           ),
           CASE
             WHEN json_extract(task.input_json, '$.acceptance_report_version') = 1
               THEN 'missing'
             ELSE 'succeeded'
           END
         ) AS result_status
  FROM local_tasks task
)
"#,
            $update
        )
    };
}

const PROPAGATE_BLOCKED_SQL: &str = dependency_gating_sql!(
    r#"UPDATE local_tasks
SET status = 'blocked',
    version = version + 1,
    updated_at_unix_ms = ?
WHERE status IN ('pending', 'ready')
  AND graph_id IN (
    SELECT graph_id
    FROM local_task_graphs
    WHERE owner_user_id = (
      SELECT owner_user_id FROM local_task_graphs WHERE graph_id = ?
    )
  )
  AND (
    EXISTS (
      SELECT 1
      FROM local_task_dependencies d
      JOIN local_tasks prerequisite
        ON prerequisite.task_id = d.prerequisite_task_id
       AND prerequisite.graph_id = d.graph_id
      JOIN prerequisite_results result ON result.task_id = prerequisite.task_id
      WHERE d.graph_id = local_tasks.graph_id
        AND d.task_id = local_tasks.task_id
        AND (
          prerequisite.status IN ('failed', 'cancelled', 'blocked')
          OR result.result_status IN ('failed', 'blocked')
        )
    )
    OR EXISTS (
      SELECT 1
      FROM local_task_external_dependencies d
      JOIN local_tasks prerequisite ON prerequisite.task_id = d.prerequisite_task_id
      JOIN prerequisite_results result ON result.task_id = prerequisite.task_id
      WHERE d.task_id = local_tasks.task_id
        AND (
          prerequisite.status IN ('failed', 'cancelled', 'blocked')
          OR result.result_status IN ('failed', 'blocked')
        )
    )
  )
"#
);

const UNLOCK_SATISFIED_SQL: &str = dependency_gating_sql!(
    r#"UPDATE local_tasks
SET status = 'ready',
    version = version + 1,
    updated_at_unix_ms = ?
WHERE status = 'pending'
  AND graph_id IN (
    SELECT graph_id
    FROM local_task_graphs
    WHERE owner_user_id = (
      SELECT owner_user_id FROM local_task_graphs WHERE graph_id = ?
    )
  )
  AND NOT EXISTS (
    SELECT 1
    FROM local_task_dependencies d
    JOIN local_tasks prerequisite
      ON prerequisite.task_id = d.prerequisite_task_id
     AND prerequisite.graph_id = d.graph_id
    JOIN prerequisite_results result ON result.task_id = prerequisite.task_id
    WHERE d.graph_id = local_tasks.graph_id
      AND d.task_id = local_tasks.task_id
      AND (
        prerequisite.status <> 'succeeded'
        OR result.result_status <> 'succeeded'
      )
  )
  AND NOT EXISTS (
    SELECT 1
    FROM local_task_external_dependencies d
    JOIN local_tasks prerequisite ON prerequisite.task_id = d.prerequisite_task_id
    JOIN prerequisite_results result ON result.task_id = prerequisite.task_id
    WHERE d.task_id = local_tasks.task_id
      AND (
        prerequisite.status <> 'succeeded'
        OR result.result_status <> 'succeeded'
      )
  )
"#
);
