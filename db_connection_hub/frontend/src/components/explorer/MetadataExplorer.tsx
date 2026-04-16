import type {
  DatabaseSummaryResponse,
  DbType,
  MetadataNode,
  ObjectStatsResponse
} from "../../types/models";
import { DatabaseObjectStatsPanel } from "./DatabaseObjectStatsPanel";
import { supportsDetailByNodeIdForDb } from "./metadataNodeUtils";

interface Props {
  datasourceId: string | null;
  activeDbType: DbType | null;
  summary: DatabaseSummaryResponse | null;
  nodes: MetadataNode[];
  nodesTotal: number;
  loading: boolean;
  loadingMore: boolean;
  canLoadMore: boolean;
  objectStats: ObjectStatsResponse | null;
  loadingObjectStats: boolean;
  currentParentId: string | null;
  onBack: () => void;
  onOpenNode: (node: MetadataNode) => void;
  onShowDetail: (node: MetadataNode) => void;
  onLoadMore: () => void;
}

export function MetadataExplorer({
  datasourceId,
  activeDbType,
  summary,
  nodes,
  nodesTotal,
  loading,
  loadingMore,
  canLoadMore,
  objectStats,
  loadingObjectStats,
  currentParentId,
  onBack,
  onOpenNode,
  onShowDetail,
  onLoadMore
}: Props) {
  return (
    <section className="card">
      <h2>Metadata Explorer</h2>
      {!datasourceId ? <p>Select a connection first.</p> : null}

      {summary ? (
        <div className="summary-row">
          <span>database_count: {summary.database_count}</span>
          <span>visible: {summary.visible_database_count}</span>
          <span>scope: {summary.visibility_scope}</span>
        </div>
      ) : null}

      {datasourceId ? (
        <DatabaseObjectStatsPanel stats={objectStats} loading={loadingObjectStats} />
      ) : null}

      {currentParentId ? (
        <button className="back-btn" onClick={onBack}>
          Back
        </button>
      ) : null}

      {loading ? <p className="hint">Loading metadata nodes...</p> : null}

      <ul className="node-list">
        {nodes.map((node) => (
          <li key={node.id}>
            <div className="actions">
              <button onClick={() => onOpenNode(node)} disabled={!node.has_children}>
                Open
              </button>
              <button
                onClick={() => onShowDetail(node)}
                disabled={!supportsDetailByNodeIdForDb(node.id, activeDbType)}
              >
                Detail
              </button>
            </div>
            <span>
              <strong>{node.display_name}</strong> · {node.node_type} · {node.path}
            </span>
          </li>
        ))}
      </ul>

      {!loading && datasourceId && nodes.length === 0 ? (
        <p className="hint">No metadata nodes in this level.</p>
      ) : null}

      {datasourceId ? (
        <div className="pager-row">
          <span>
            loaded: {nodes.length} / {nodesTotal}
          </span>
          {canLoadMore ? (
            <button className="back-btn" onClick={onLoadMore} disabled={loadingMore}>
              {loadingMore ? "Loading..." : "Load more"}
            </button>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
