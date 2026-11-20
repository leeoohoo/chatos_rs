use super::*;

impl AgentConversationStore {
    pub(crate) fn communication_rooms_requiring_scheduler(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<String>, String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT DISTINCT room.id
                FROM project_agent_rooms room
                JOIN project_agent_deliveries delivery
                  ON delivery.owner_user_id = room.owner_user_id
                 AND delivery.room_id = room.id
                LEFT JOIN windows_agent_delivery_runs run
                  ON run.owner_user_id = delivery.owner_user_id
                 AND run.delivery_id = delivery.id
                WHERE room.owner_user_id = ?1 AND room.status = 'active'
                  AND (room.conversation_kind = 'agent_agent_direct'
                       OR (room.conversation_kind = 'human_agent_direct'
                           AND delivery.trigger_kind IN ('heartbeat', 'mention')))
                  AND (delivery.status IN ('pending', 'running')
                       OR run.status IN ('ready', 'running'))
                ORDER BY room.id
                LIMIT 500
                "#,
            )
            .map_err(|_| team_todo_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id], |row| row.get::<_, String>(0))
            .map_err(|_| team_todo_store_error())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_todo_store_error())
    }

    pub(crate) fn communication_scheduler_has_work(
        &self,
        owner_user_id: &str,
        room_id: &str,
    ) -> Result<bool, String> {
        validate_identifier(room_id).map_err(|_| invalid_team_todo_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        connection
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1
                    FROM project_agent_deliveries delivery
                    JOIN project_agent_rooms room
                      ON room.owner_user_id = delivery.owner_user_id
                     AND room.id = delivery.room_id
                    LEFT JOIN windows_agent_delivery_runs run
                      ON run.owner_user_id = delivery.owner_user_id
                     AND run.delivery_id = delivery.id
                    WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                      AND room.status = 'active'
                      AND (room.conversation_kind = 'agent_agent_direct'
                           OR (room.conversation_kind = 'human_agent_direct'
                               AND delivery.trigger_kind IN ('heartbeat', 'mention')))
                      AND (delivery.status IN ('pending', 'running')
                           OR run.status IN ('ready', 'running'))
                )
                "#,
                params![owner_user_id, room_id],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())
    }

    pub(crate) fn teams_requiring_todo_scheduler(
        &self,
        owner_user_id: &str,
    ) -> Result<Vec<String>, String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT room.id
                FROM project_agent_rooms room
                WHERE room.owner_user_id = ?1 AND room.status = 'active'
                  AND room.conversation_kind = 'project_team'
                  AND (
                    EXISTS (
                        SELECT 1
                        FROM project_agent_deliveries delivery
                        LEFT JOIN windows_agent_delivery_runs run
                          ON run.owner_user_id = delivery.owner_user_id
                         AND run.delivery_id = delivery.id
                        WHERE delivery.owner_user_id = room.owner_user_id
                          AND delivery.room_id = room.id
                          AND (delivery.status IN ('pending', 'running')
                               OR run.status IN ('ready', 'running'))
                    )
                    OR EXISTS (
                        SELECT 1
                        FROM local_agent_todos todo
                        JOIN project_agent_room_members member
                          ON member.owner_user_id = todo.owner_user_id
                         AND member.room_id = todo.team_room_id
                         AND member.agent_id = todo.agent_id
                        JOIN local_agent_profiles profile
                          ON profile.owner_user_id = member.owner_user_id
                         AND profile.id = member.agent_id
                        WHERE todo.owner_user_id = room.owner_user_id
                          AND todo.team_room_id = room.id AND todo.status = 'pending'
                          AND member.status = 'active' AND profile.status = 'active'
                          AND NOT EXISTS (
                            SELECT 1 FROM local_agent_todos running
                            WHERE running.owner_user_id = todo.owner_user_id
                              AND running.agent_id = todo.agent_id
                              AND running.status = 'in_progress'
                          )
                          AND NOT EXISTS (
                            SELECT 1
                            FROM local_agent_todo_dependencies dependency
                            JOIN local_agent_todos prerequisite
                              ON prerequisite.owner_user_id = dependency.owner_user_id
                             AND prerequisite.id = dependency.prerequisite_todo_id
                            WHERE dependency.owner_user_id = todo.owner_user_id
                              AND dependency.todo_id = todo.id
                              AND prerequisite.status != 'completed'
                          )
                    )
                  )
                ORDER BY room.id
                LIMIT 500
                "#,
            )
            .map_err(|_| team_todo_store_error())?;
        let rows = statement
            .query_map(params![owner_user_id], |row| row.get::<_, String>(0))
            .map_err(|_| team_todo_store_error())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| team_todo_store_error())
    }

    pub(crate) fn team_todo_scheduler_has_work(
        &self,
        owner_user_id: &str,
        team_id: &str,
    ) -> Result<bool, String> {
        validate_identifier(team_id).map_err(|_| invalid_team_todo_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_active_project_team(&connection, owner_user_id, team_id)?;
        connection
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1
                    FROM project_agent_deliveries delivery
                    LEFT JOIN windows_agent_delivery_runs run
                      ON run.owner_user_id = delivery.owner_user_id
                     AND run.delivery_id = delivery.id
                    WHERE delivery.owner_user_id = ?1 AND delivery.room_id = ?2
                      AND (delivery.status IN ('pending', 'running')
                           OR run.status IN ('ready', 'running'))
                )
                "#,
                params![owner_user_id, team_id],
                |row| row.get(0),
            )
            .map_err(|_| team_todo_store_error())
    }
}
