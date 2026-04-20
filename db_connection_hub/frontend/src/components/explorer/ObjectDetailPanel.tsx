import type { ObjectDetailResponse } from "../../types/models";

interface Props {
  detail: ObjectDetailResponse | null;
}

export function ObjectDetailPanel({ detail }: Props) {
  return (
    <section className="card object-detail-card">
      <h2>Object Detail</h2>
      {!detail ? <p>Select "Detail" on a node to inspect columns/indexes/constraints.</p> : null}

      {detail ? (
        <>
          <div className="summary-row">
            <span>name: {detail.name}</span>
            <span>type: {detail.node_type}</span>
            <span>node_id: {detail.node_id}</span>
          </div>

          <h3>Columns</h3>
          <ul className="simple-list">
            {detail.columns.length === 0 ? <li>No columns</li> : null}
            {detail.columns.map((column) => (
              <li key={column.name}>
                <strong>{column.name}</strong>
                <span>
                  {column.data_type} · nullable: {String(column.nullable)}
                </span>
              </li>
            ))}
          </ul>

          <h3>Indexes</h3>
          <ul className="simple-list">
            {detail.indexes.length === 0 ? <li>No indexes</li> : null}
            {detail.indexes.map((index) => (
              <li key={index.name}>
                <strong>{index.name}</strong>
                <span>
                  unique: {String(index.is_unique)} · columns: {index.columns.join(", ") || "-"}
                </span>
              </li>
            ))}
          </ul>

          <h3>Constraints</h3>
          <ul className="simple-list">
            {detail.constraints.length === 0 ? <li>No constraints</li> : null}
            {detail.constraints.map((constraint) => (
              <li key={constraint.name}>
                <strong>{constraint.name}</strong>
                <span>
                  {constraint.constraint_type} · columns: {constraint.columns.join(", ") || "-"}
                </span>
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </section>
  );
}
