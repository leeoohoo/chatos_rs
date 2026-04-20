import type { ObjectStatsResponse } from "../../types/models";

interface Props {
  stats: ObjectStatsResponse | null;
  loading: boolean;
}

const STAT_FIELDS: Array<{ key: keyof ObjectStatsResponse; label: string }> = [
  { key: "schema_count", label: "schemas" },
  { key: "table_count", label: "tables" },
  { key: "view_count", label: "views" },
  { key: "materialized_view_count", label: "materialized_views" },
  { key: "collection_count", label: "collections" },
  { key: "index_count", label: "indexes" },
  { key: "procedure_count", label: "procedures" },
  { key: "function_count", label: "functions" },
  { key: "trigger_count", label: "triggers" },
  { key: "sequence_count", label: "sequences" },
  { key: "synonym_count", label: "synonyms" },
  { key: "package_count", label: "packages" }
];

export function DatabaseObjectStatsPanel({ stats, loading }: Props) {
  if (loading) {
    return <p className="hint">Loading database object stats...</p>;
  }

  if (!stats) {
    return <p className="hint">Open a database node to view object stats.</p>;
  }

  return (
    <div className="stats-panel">
      <div className="stats-panel__header">
        <strong>database: {stats.database}</strong>
        <span>partial: {String(stats.partial)}</span>
      </div>
      <ul className="stats-list">
        {STAT_FIELDS.filter((field) => typeof stats[field.key] === "number").map((field) => (
          <li key={field.key}>
            <span>{field.label}</span>
            <strong>{stats[field.key] as number}</strong>
          </li>
        ))}
      </ul>
    </div>
  );
}
