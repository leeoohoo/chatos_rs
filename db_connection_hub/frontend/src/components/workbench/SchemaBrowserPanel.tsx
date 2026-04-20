import type { DatabaseInfo, MetadataNode } from "../../types/models";

interface Props {
  datasourceName: string | null;
  loadingDatabases: boolean;
  databases: DatabaseInfo[];
  selectedDatabase: string | null;
  onSelectDatabase: (database: string) => void;
  loadingObjects: boolean;
  objectPath: string[];
  objects: MetadataNode[];
  selectedNodeId: string | null;
  onBackObject: () => void;
  onOpenObject: (node: MetadataNode) => void;
}

export function SchemaBrowserPanel({
  datasourceName,
  loadingDatabases,
  databases,
  selectedDatabase,
  onSelectDatabase,
  loadingObjects,
  objectPath,
  objects,
  selectedNodeId,
  onBackObject,
  onOpenObject
}: Props) {
  const canBack = objectPath.length > 1;

  return (
    <section className="workbench-column browser-column">
      <div className="column-header">
        <h2>Browser</h2>
        <span>{datasourceName || "No Connection"}</span>
      </div>

      <div className="browser-pane">
        <h3>Databases</h3>
        {loadingDatabases ? <p className="hint">Loading databases...</p> : null}
        <ul className="sidebar-list">
          {!loadingDatabases && databases.length === 0 ? (
            <li className="empty-row">No databases found.</li>
          ) : null}
          {databases.map((database) => (
            <li
              key={database.name}
              className={`sidebar-row ${selectedDatabase === database.name ? "active" : ""}`}
              onClick={() => onSelectDatabase(database.name)}
            >
              <div className="sidebar-row-main">
                <strong>{database.name}</strong>
                <span>database</span>
              </div>
            </li>
          ))}
        </ul>
      </div>

      <div className="browser-pane">
        <div className="column-header inline">
          <h3>Objects</h3>
          {canBack ? (
            <button className="ghost-btn" onClick={onBackObject}>
              Back
            </button>
          ) : null}
        </div>
        <p className="hint">Path: {objectPath.join(" / ") || "-"}</p>
        {loadingObjects ? <p className="hint">Loading objects...</p> : null}
        <ul className="sidebar-list">
          {!loadingObjects && objects.length === 0 ? <li className="empty-row">No objects.</li> : null}
          {objects.map((node) => (
            <li
              key={node.id}
              className={`sidebar-row ${selectedNodeId === node.id ? "active" : ""}`}
              onClick={() => onOpenObject(node)}
            >
              <div className="sidebar-row-main">
                <strong>{node.display_name}</strong>
                <span>
                  {node.node_type}
                  {node.has_children ? " · folder" : ""}
                </span>
              </div>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
