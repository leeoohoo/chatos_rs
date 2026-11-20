export function formatTeamMessageTime(unixMs: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit",
  }).format(new Date(unixMs));
}

export function Row({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd title={value}>{value}</dd></div>;
}

export function Pagination({
  page,
  pageCount,
  pageSize,
  total,
  onPageChange,
  onPageSizeChange,
}: {
  page: number;
  pageCount: number;
  pageSize: number;
  total: number;
  onPageChange(page: number): void;
  onPageSizeChange(size: number): void;
}) {
  const start = total === 0 ? 0 : page * pageSize + 1;
  const end = Math.min(total, start + pageSize - 1);
  return (
    <footer className="agent-pagination">
      <span>{start}–{end} / {total}</span>
      <div>
        <label>
          <span className="sr-only">每页</span>
          <select aria-label="每页" onChange={(event) => onPageSizeChange(Number(event.target.value))} value={pageSize}>
            {[10, 20, 50].map((size) => <option key={size} value={size}>{size} / 页</option>)}
          </select>
        </label>
        <button aria-label="上一页" disabled={page === 0} onClick={() => onPageChange(page - 1)} type="button">‹</button>
        <span>{page + 1} / {pageCount}</span>
        <button aria-label="下一页" disabled={page + 1 >= pageCount} onClick={() => onPageChange(page + 1)} type="button">›</button>
      </div>
    </footer>
  );
}
