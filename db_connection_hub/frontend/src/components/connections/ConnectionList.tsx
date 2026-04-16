import { useState } from "react";
import type { ConnectionTestResult, DataSourceListItem } from "../../types/models";
import { ConnectionTestResultCard } from "./ConnectionTestResultCard";

interface Props {
  items: DataSourceListItem[];
  activeId: string | null;
  testResult: ConnectionTestResult | null;
  onSelect: (id: string) => void;
  onTest: (id: string) => Promise<void>;
  onEditName: (id: string, name: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
}

export function ConnectionList({
  items,
  activeId,
  testResult,
  onSelect,
  onTest,
  onEditName,
  onDelete
}: Props) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState("");
  const [updatingId, setUpdatingId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  function beginEdit(item: DataSourceListItem) {
    setEditingId(item.id);
    setEditingName(item.name);
  }

  function cancelEdit() {
    setEditingId(null);
    setEditingName("");
  }

  async function saveEdit(id: string) {
    const name = editingName.trim();
    if (name === "") {
      return;
    }

    setUpdatingId(id);
    try {
      await onEditName(id, name);
      if (editingId === id) {
        cancelEdit();
      }
    } finally {
      setUpdatingId(null);
    }
  }

  async function removeItem(item: DataSourceListItem) {
    if (!window.confirm(`Delete connection "${item.name}"?`)) {
      return;
    }

    setDeletingId(item.id);
    try {
      await onDelete(item.id);
      if (editingId === item.id) {
        cancelEdit();
      }
    } finally {
      setDeletingId(null);
    }
  }

  return (
    <section className="card">
      <h2>Connections</h2>
      {items.length === 0 ? <p>No connections yet.</p> : null}

      <ul className="connection-list">
        {items.map((item) => (
          <li key={item.id} className={activeId === item.id ? "active" : ""}>
            <div>
              {editingId === item.id ? (
                <input
                  className="connection-name-input"
                  value={editingName}
                  onChange={(event) => setEditingName(event.target.value)}
                  placeholder="Connection name"
                  disabled={updatingId === item.id}
                />
              ) : (
                <strong>{item.name}</strong>
              )}
              <span>
                {item.db_type} · {item.status}
              </span>
            </div>
            <div className="actions">
              <button
                onClick={() => onSelect(item.id)}
                disabled={updatingId === item.id || deletingId === item.id}
              >
                Select
              </button>
              <button
                onClick={() => onTest(item.id)}
                disabled={updatingId === item.id || deletingId === item.id}
              >
                Test
              </button>
              {editingId === item.id ? (
                <>
                  <button
                    onClick={() => void saveEdit(item.id)}
                    disabled={
                      updatingId === item.id ||
                      deletingId === item.id ||
                      editingName.trim() === ""
                    }
                  >
                    Save
                  </button>
                  <button
                    onClick={cancelEdit}
                    disabled={updatingId === item.id || deletingId === item.id}
                  >
                    Cancel
                  </button>
                </>
              ) : (
                <button
                  onClick={() => beginEdit(item)}
                  disabled={updatingId === item.id || deletingId === item.id}
                >
                  Edit
                </button>
              )}
              <button
                className="danger-btn"
                onClick={() => void removeItem(item)}
                disabled={updatingId === item.id || deletingId === item.id}
              >
                Delete
              </button>
            </div>
          </li>
        ))}
      </ul>

      {testResult ? <ConnectionTestResultCard testResult={testResult} /> : null}
    </section>
  );
}
