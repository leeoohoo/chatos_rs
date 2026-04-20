import { ObjectDetailPanel } from "../explorer/ObjectDetailPanel";
import { SqlConsole } from "./SqlConsole";
import type { ObjectDetailResponse, QueryExecuteResponse } from "../../types/models";

interface Props {
  activeTab: "detail" | "sql";
  onChangeTab: (tab: "detail" | "sql") => void;
  detail: ObjectDetailResponse | null;
  disabled: boolean;
  selectedDatabase: string | null;
  sql: string;
  loading: boolean;
  result: QueryExecuteResponse | null;
  onSqlChange: (value: string) => void;
  onExecute: () => Promise<void>;
}

export function InspectorTabs({
  activeTab,
  onChangeTab,
  detail,
  disabled,
  selectedDatabase,
  sql,
  loading,
  result,
  onSqlChange,
  onExecute
}: Props) {
  return (
    <section className="inspector-tabs-panel">
      <div className="inspector-tabs-header">
        <button
          type="button"
          className={`tab-btn ${activeTab === "detail" ? "active" : ""}`}
          onClick={() => onChangeTab("detail")}
        >
          Table Detail
        </button>
        <button
          type="button"
          className={`tab-btn ${activeTab === "sql" ? "active" : ""}`}
          onClick={() => onChangeTab("sql")}
        >
          SQL Query
        </button>
      </div>

      <div className="inspector-tab-body">
        {activeTab === "detail" ? (
          <ObjectDetailPanel detail={detail} />
        ) : (
          <SqlConsole
            disabled={disabled}
            selectedDatabase={selectedDatabase}
            sql={sql}
            loading={loading}
            result={result}
            onSqlChange={onSqlChange}
            onExecute={onExecute}
          />
        )}
      </div>
    </section>
  );
}
