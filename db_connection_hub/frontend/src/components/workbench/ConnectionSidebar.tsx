import { useState } from "react";
import type { DataSourceListItem } from "../../types/models";

interface Props {
  items: DataSourceListItem[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onEdit: (id: string) => void;
  onDelete: (id: string) => Promise<void>;
  onTest: (id: string) => Promise<void>;
}

export function ConnectionSidebar({
  items,
  activeId,
  onSelect,
  onCreate,
  onEdit,
  onDelete,
  onTest
}: Props) {
  const [testingId, setTestingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  async function handleTest(id: string) {
    setTestingId(id);
    try {
      await onTest(id);
    } finally {
      setTestingId(null);
    }
  }

  async function handleDelete(item: DataSourceListItem) {
    if (!window.confirm(`Delete connection "${item.name}"?`)) {
      return;
    }
    setDeletingId(item.id);
    try {
      await onDelete(item.id);
    } finally {
      setDeletingId(null);
    }
  }

  return (
    <aside className="workbench-column sidebar-column">
      <div className="column-header">
        <h2>Connections</h2>
        <button onClick={onCreate}>+ New</button>
      </div>

      <ul className="sidebar-list">
        {items.length === 0 ? <li className="empty-row">No connections yet.</li> : null}
        {items.map((item) => (
          <li
            key={item.id}
            className={`sidebar-row ${activeId === item.id ? "active" : ""}`}
            onClick={() => onSelect(item.id)}
          >
            <div className="sidebar-row-main">
              <strong>{item.name}</strong>
              <span>
                {item.db_type} · <em className={`status-${item.status}`}>{item.status}</em>
              </span>
            </div>
            <div className="sidebar-row-actions">
              <button
                className="ghost-btn"
                onClick={(event) => {
                  event.stopPropagation();
                  void handleTest(item.id);
                }}
                disabled={testingId === item.id || deletingId === item.id}
              >
                {testingId === item.id ? "..." : "Test"}
              </button>
              <button
                className="ghost-btn"
                onClick={(event) => {
                  event.stopPropagation();
                  onEdit(item.id);
                }}
                disabled={testingId === item.id || deletingId === item.id}
              >
                Edit
              </button>
              <button
                className="danger-btn"
                onClick={(event) => {
                  event.stopPropagation();
                  void handleDelete(item);
                }}
                disabled={testingId === item.id || deletingId === item.id}
              >
                Del
              </button>
            </div>
          </li>
        ))}
      </ul>
    </aside>
  );
}
