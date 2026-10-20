import { useLayoutEffect, useState } from "react";
import { useRemoteFileStore } from "../application/remoteFileStore";
import { visibleRemoteEntries, formatRemoteFileSize, type RemoteFileEntry } from "../domain/remoteFiles";
import type { RemoteFilePort } from "../ports/RemoteFilePort";
import { RemoteFileBrowser } from "./RemoteFileBrowser";
import { RemoteFileConfirmDialog, RemoteFileTextDialog, RemoteFileVerificationDialog } from "./RemoteFileDialogs";
import "./remoteFiles.css";

type DialogState =
  | { kind: "create"; value: string }
  | { kind: "rename"; entry: RemoteFileEntry; value: string }
  | { kind: "delete"; entry: RemoteFileEntry }
  | { kind: "upload-overwrite" }
  | { kind: "download-overwrite" }
  | null;

export function RemoteSftpWorkspace({ ownerUserId, connectionId, port }: { ownerUserId: string; connectionId: string; port: RemoteFilePort }) {
  const state = useRemoteFileStore();
  const [dialog, setDialog] = useState<DialogState>(null);
  const busy = state.activeOperation != null;
  const entries = visibleRemoteEntries(state.listing?.entries ?? [], state.searchText, state.showsHiddenFiles);
  const selectedEntry = state.listing?.entries.find((entry) => entry.path === state.selectedRemotePath) ?? null;

  useLayoutEffect(() => {
    void useRemoteFileStore.getState().activate(port, ownerUserId, connectionId);
    return () => useRemoteFileStore.getState().deactivate(port);
  }, [connectionId, ownerUserId, port]);

  const open = (entry: RemoteFileEntry) => {
    if (entry.kind === "directory") void state.loadDirectory(port, entry.path);
    else state.selectRemote(entry.path);
  };
  const beginUpload = () => {
    if (!state.selectedUpload || !state.listing) return;
    if (state.listing.entries.some((entry) => entry.name === state.selectedUpload?.fileName)) setDialog({ kind: "upload-overwrite" });
    else void state.uploadSelected(port, false);
  };
  const beginDownload = async () => {
    if (!selectedEntry || selectedEntry.kind !== "file") return;
    if (state.pendingDownload?.remotePath === selectedEntry.path) {
      setDialog({ kind: "download-overwrite" });
      return;
    }
    if (await state.prepareDownload(port, selectedEntry)) setDialog({ kind: "download-overwrite" });
  };

  return (
    <section className="remote-sftp" aria-label="SFTP 文件工作区">
      <header className="remote-sftp__toolbar">
        <strong><span aria-hidden="true">▰</span>SFTP</strong>
        <label><span aria-hidden="true">⌕</span><input aria-label="筛选文件" disabled={busy} onChange={(event) => state.setSearchText(event.target.value)} placeholder="筛选文件" type="search" value={state.searchText} /></label>
        <label className="remote-sftp__hidden"><input checked={state.showsHiddenFiles} disabled={busy} onChange={(event) => state.setShowsHiddenFiles(event.target.checked)} type="checkbox" />显示隐藏文件</label>
      </header>
      <Feedback />
      <div className="remote-sftp__panes">
        <aside className="remote-sftp-local">
          <header><div><strong>本机</strong><small>路径由 Windows 原生选择器保管</small></div><button disabled={busy} onClick={() => void state.pickUploadFile(port)} type="button">选择文件…</button></header>
          <div className="remote-sftp-local__selection">
            <span aria-hidden="true">{state.selectedUpload ? "▤" : "＋"}</span>
            {state.selectedUpload ? <><strong>{state.selectedUpload.fileName}</strong><small>{formatRemoteFileSize(state.selectedUpload.size)}</small></> : <><strong>选择本机文件</strong><small>页面只能看到文件名和大小，不会读取本机绝对路径。</small></>}
          </div>
          <footer><span>{state.selectedUpload?.fileName ?? "尚未选择文件"}</span><button className="primary-button" disabled={busy || !state.selectedUpload || !state.listing} onClick={beginUpload} type="button">上传到服务器 →</button></footer>
        </aside>
        <div className="remote-sftp-remote">
          <header className="remote-sftp-pathbar">
            <div><strong>服务器</strong><code>{state.listing?.path ?? "正在连接…"}</code></div>
            <span />
            <button aria-label="返回上级目录" disabled={busy || !state.listing?.parentPath} onClick={() => void state.loadDirectory(port, state.listing?.parentPath ?? null)} type="button">↑</button>
            <button aria-label="刷新远端目录" disabled={busy || !state.listing} onClick={() => void state.loadDirectory(port, state.listing?.path ?? null)} type="button">↻</button>
            <button aria-label="新建远端目录" disabled={busy || !state.listing} onClick={() => setDialog({ kind: "create", value: "" })} type="button">＋</button>
            <button disabled={busy || !selectedEntry} onClick={() => selectedEntry && setDialog({ kind: "rename", entry: selectedEntry, value: selectedEntry.name })} type="button">重命名</button>
            <button className="danger-link" disabled={busy || !selectedEntry} onClick={() => selectedEntry && setDialog({ kind: "delete", entry: selectedEntry })} type="button">删除</button>
          </header>
          <RemoteFileBrowser entries={entries} loading={state.activeOperation?.kind === "list"} selectedPath={state.selectedRemotePath} onOpen={open} onSelect={(entry) => state.selectRemote(entry.path)} />
          <footer className="remote-sftp-transfer"><span>{selectedEntry?.name ?? "选择远端文件"}</span><button className="primary-button" disabled={busy || selectedEntry?.kind !== "file"} onClick={() => void beginDownload()} type="button">← 下载到本机</button></footer>
        </div>
      </div>
      <Dialogs dialog={dialog} setDialog={setDialog} selectedEntry={selectedEntry} port={port} />
    </section>
  );
}

function Feedback() {
  const state = useRemoteFileStore();
  const text = state.errorMessage ?? state.refreshWarning ?? state.notice;
  if (!text && !state.listing?.truncated && !state.activeOperation) return null;
  const isError = Boolean(state.errorMessage || state.refreshWarning || state.listing?.truncated);
  return (
    <div className={`remote-sftp__feedback ${isError ? "is-warning" : ""}`} role={state.errorMessage ? "alert" : "status"}>
      {state.activeOperation && <span className="mini-spinner" />}
      <span>{text ?? (state.listing?.truncated ? "远端目录项目过多，仅显示前 1000 项。" : operationLabel(state.activeOperation?.kind))}</span>
      {text && <button aria-label="关闭提示" onClick={state.dismissFeedback} type="button">×</button>}
    </div>
  );
}

function Dialogs({ dialog, setDialog, selectedEntry, port }: { dialog: DialogState; setDialog(value: DialogState): void; selectedEntry: RemoteFileEntry | null; port: RemoteFilePort }) {
  const state = useRemoteFileStore();
  const busy = state.activeOperation != null;
  if (state.verificationPrompt) return <RemoteFileVerificationDialog busy={busy} code={state.verificationCode} errorMessage={state.errorMessage} prompt={state.verificationPrompt} onCancel={() => void state.cancelVerification(port)} onCodeChange={state.setVerificationCode} onSubmit={() => void state.submitVerification(port)} />;
  if (!dialog) return null;
  if (dialog.kind === "create") return <RemoteFileTextDialog busy={busy} confirmLabel="创建" description={`在 ${state.listing?.path ?? "当前目录"} 中创建文件夹。`} title="新建远端目录" value={dialog.value} onCancel={() => setDialog(null)} onConfirm={() => { void state.createDirectory(port, dialog.value); setDialog(null); }} onValueChange={(value) => setDialog({ ...dialog, value })} />;
  if (dialog.kind === "rename") return <RemoteFileTextDialog busy={busy} confirmLabel="保存" description={`重命名 ${dialog.entry.name}`} title="重命名" value={dialog.value} onCancel={() => setDialog(null)} onConfirm={() => { void state.renameEntry(port, dialog.entry, dialog.value); setDialog(null); }} onValueChange={(value) => setDialog({ ...dialog, value })} />;
  if (dialog.kind === "delete") return <RemoteFileConfirmDialog busy={busy} confirmLabel="删除" danger description={`此操作会直接删除服务器上的“${dialog.entry.name}”${dialog.entry.kind === "directory" ? "及其全部内容" : ""}，无法撤销。`} title="删除远端项目？" onCancel={() => setDialog(null)} onConfirm={() => { void state.deleteEntry(port, dialog.entry); setDialog(null); }} />;
  if (dialog.kind === "upload-overwrite") return <RemoteFileConfirmDialog busy={busy} confirmLabel="覆盖" danger description={`服务器上已存在“${state.selectedUpload?.fileName ?? "同名文件"}”，继续将覆盖它。`} title="覆盖远端文件？" onCancel={() => setDialog(null)} onConfirm={() => { void state.uploadSelected(port, true); setDialog(null); }} />;
  return <RemoteFileConfirmDialog busy={busy} confirmLabel="保存并覆盖" description={`将“${selectedEntry?.name ?? "远端文件"}”保存到你选择的位置；如果目标文件已存在，将直接覆盖。`} title="保存下载文件？" onCancel={() => { void state.discardPreparedSelections(port); setDialog(null); }} onConfirm={() => { void state.downloadPrepared(port, true); setDialog(null); }} />;
}

function operationLabel(kind?: string) {
  if (kind === "pick_upload" || kind === "pick_download") return "正在等待本机文件选择…";
  if (kind === "upload") return "正在上传文件…";
  if (kind === "download") return "正在下载文件…";
  if (kind === "list") return "正在读取远端目录…";
  return "正在执行远端文件操作…";
}
