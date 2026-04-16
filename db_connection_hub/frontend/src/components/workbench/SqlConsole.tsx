import type { QueryExecuteResponse } from "../../types/models";

interface Props {
  disabled: boolean;
  selectedDatabase: string | null;
  sql: string;
  loading: boolean;
  result: QueryExecuteResponse | null;
  onSqlChange: (value: string) => void;
  onExecute: () => Promise<void>;
}

export function SqlConsole({
  disabled,
  selectedDatabase,
  sql,
  loading,
  result,
  onSqlChange,
  onExecute
}: Props) {
  return (
    <section className="card sql-console">
      <div className="column-header inline">
        <h2>SQL Console</h2>
        <span>{selectedDatabase ? `DB: ${selectedDatabase}` : "DB: auto/default"}</span>
      </div>

      <textarea
        className="sql-textarea"
        value={sql}
        onChange={(event) => onSqlChange(event.target.value)}
        placeholder="SELECT * FROM your_table LIMIT 100;"
        disabled={disabled || loading}
      />
      <div className="sql-actions">
        <button onClick={() => void onExecute()} disabled={disabled || loading || sql.trim() === ""}>
          {loading ? "Running..." : "Run SQL"}
        </button>
      </div>

      {result ? (
        <div className="sql-result">
          <p className="hint">
            query_id: {result.query_id} · rows: {result.row_count} · elapsed: {result.elapsed_ms} ms
          </p>
          {result.columns.length > 0 ? (
            <div className="result-table-wrap">
              <table className="result-table">
                <thead>
                  <tr>
                    {result.columns.map((column) => (
                      <th key={column.name}>{column.name}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {result.rows.length === 0 ? (
                    <tr>
                      <td colSpan={result.columns.length}>No rows returned.</td>
                    </tr>
                  ) : null}
                  {result.rows.map((row, rowIndex) => (
                    <tr key={`r-${rowIndex}`}>
                      {row.map((cell, cellIndex) => (
                        <td key={`c-${rowIndex}-${cellIndex}`}>{formatCell(cell)}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <p className="hint">Mutation succeeded. Affected rows: {result.row_count}</p>
          )}
        </div>
      ) : (
        <p className="hint">Run a SQL statement to view result rows.</p>
      )}
    </section>
  );
}

function formatCell(value: unknown): string {
  if (value === null || value === undefined) {
    return "NULL";
  }
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}
