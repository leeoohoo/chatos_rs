import React from 'react';

import { RowsCard, renderCardHeader } from '../shared/primitives';
import { asArray, asBoolean, asNumber, asRecord, asString } from '../shared/value';

const TaskListCard: React.FC<{ title: string; items: unknown[] }> = ({ title, items }) => {
  const tasks = items
    .map((item) => asRecord(item))
    .filter((item): item is Record<string, unknown> => item !== null);

  if (tasks.length === 0) return null;

  return (
    <div className="tool-detail-card tool-detail-card--full">
      {renderCardHeader(title, `${tasks.length} 项`)}
      <div className="tool-detail-list">
        {tasks.map((task, index) => {
          const titleText = asString(task.title).trim() || `task ${index + 1}`;
          const details = asString(task.details).trim();
          const priority = asString(task.priority).trim();
          const status = asString(task.status).trim();
          const dueAt = asString(task.due_at ?? task.dueAt).trim();
          const tags = asArray(task.tags).map((item) => asString(item).trim()).filter(Boolean);

          return (
            <div key={`task-${index}`} className="tool-detail-item">
              <div className="tool-detail-item-title">{titleText}</div>
              <div className="tool-detail-item-meta">
                {[priority, status, dueAt].filter(Boolean).join(' · ')}
              </div>
              {(details || tags.length > 0) && (
                <div className="tool-detail-item-body">
                  {[details, tags.length > 0 ? `#${tags.join(' #')}` : ''].filter(Boolean).join(' · ')}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};

interface TaskManagerToolDetailsProps {
  displayName: string;
  result: unknown;
}

export const TaskManagerToolDetails: React.FC<TaskManagerToolDetailsProps> = ({
  displayName,
  result,
}) => {
  const record = asRecord(result);
  if (!record) return null;

  if (displayName === 'add_task') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Review result"
          rows={[
            { key: 'confirmed', value: asBoolean(record.confirmed) },
            { key: 'cancelled', value: asBoolean(record.cancelled) },
            { key: 'created count', value: asNumber(record.created_count ?? record.createdCount) },
            { key: 'reason', value: asString(record.reason).trim() },
          ]}
        />
        <TaskListCard title="Tasks" items={asArray(record.tasks)} />
      </div>
    );
  }

  if (displayName === 'list_tasks') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Task scope"
          rows={[
            { key: 'count', value: asNumber(record.count) },
            { key: 'current turn only', value: asString(record.conversation_turn_id ?? record.conversationTurnId).trim() ? 'yes' : 'no' },
          ]}
        />
        <TaskListCard title="Tasks" items={asArray(record.tasks)} />
      </div>
    );
  }

  if (displayName === 'update_task' || displayName === 'complete_task') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title={displayName === 'complete_task' ? 'Completion result' : 'Update result'}
          rows={[
            { key: 'updated', value: asBoolean(record.updated) },
            { key: 'completed', value: asBoolean(record.completed) },
          ]}
        />
        <TaskListCard title="Task" items={[record.task]} />
      </div>
    );
  }

  if (displayName === 'delete_task') {
    return (
      <div className="tool-detail-stack">
        <RowsCard
          title="Delete result"
          rows={[
            { key: 'deleted', value: asBoolean(record.deleted) },
            { key: 'task id', value: asString(record.task_id ?? record.taskId).trim() },
            { key: 'reason', value: asString(record.reason).trim() },
          ]}
        />
      </div>
    );
  }

  return null;
};

export default TaskManagerToolDetails;

