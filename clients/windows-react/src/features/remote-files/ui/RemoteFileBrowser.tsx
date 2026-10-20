import { formatRemoteFileSize, type RemoteFileEntry } from "../domain/remoteFiles";

export function RemoteFileBrowser({
  entries,
  loading,
  selectedPath,
  onOpen,
  onSelect,
}: {
  entries: RemoteFileEntry[];
  loading: boolean;
  selectedPath: string | null;
  onOpen(entry: RemoteFileEntry): void;
  onSelect(entry: RemoteFileEntry): void;
}) {
  if (loading && entries.length === 0) {
    return <div className="remote-file-browser__empty"><span className="mini-spinner" />正在读取远端目录…</div>;
  }
  if (entries.length === 0) {
    return <div className="remote-file-browser__empty">这个远端目录为空</div>;
  }
  return (
    <div className="remote-file-browser" role="listbox" aria-label="远端文件">
      <div className="remote-file-browser__columns" aria-hidden="true"><span>名称</span><span>大小</span><span>修改时间</span></div>
      {entries.map((entry) => (
        <button
          aria-selected={selectedPath === entry.path}
          className={selectedPath === entry.path ? "is-selected" : ""}
          key={entry.path}
          onClick={() => onSelect(entry)}
          onDoubleClick={() => onOpen(entry)}
          role="option"
          type="button"
        >
          <span className="remote-file-browser__name"><i aria-hidden="true">{entry.kind === "directory" ? "▰" : entry.kind === "file" ? "▤" : "⌁"}</i><strong>{entry.name}</strong><small>{entry.permissions ?? ""}</small></span>
          <span>{formatRemoteFileSize(entry.size)}</span>
          <span>{formatTimestamp(entry.modifiedAtEpochSeconds)}</span>
        </button>
      ))}
    </div>
  );
}

function formatTimestamp(value: number | null) {
  if (value == null) return "—";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value * 1_000));
}
