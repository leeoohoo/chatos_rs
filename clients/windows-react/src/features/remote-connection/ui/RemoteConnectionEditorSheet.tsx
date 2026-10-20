import { useEffect, useLayoutEffect, useRef } from "react";
import { useRemoteConnectionEditorStore } from "../application/remoteConnectionEditorStore";
import type { RemoteConnection } from "../domain/remoteConnection";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";
import { RemoteConnectionForm } from "./RemoteConnectionForm";
import { RemoteVerificationDialog } from "./RemoteVerificationDialog";
import "./remoteConnection.css";

export function RemoteConnectionEditorSheet({
  connections,
  editingConnection,
  ownerUserId,
  port,
  onDismiss,
  onSaved,
}: {
  connections: RemoteConnection[];
  editingConnection: RemoteConnection | null;
  ownerUserId: string;
  port: RemoteConnectionPort;
  onDismiss(): void;
  onSaved(connection: RemoteConnection): void;
}) {
  const state = useRemoteConnectionEditorStore();
  const busy = state.activeOperation != null;
  const initialPresentation = useRef({ connections, editingConnection });

  useLayoutEffect(() => {
    const initial = initialPresentation.current;
    useRemoteConnectionEditorStore.getState().open(
      port,
      ownerUserId,
      initial.editingConnection,
      initial.connections,
    );
    return () => useRemoteConnectionEditorStore.getState().close(port);
  }, [ownerUserId, port]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const current = useRemoteConnectionEditorStore.getState();
      if (event.key !== "Escape" || current.activeOperation) return;
      if (current.verificationPrompt) current.dismissVerification();
      else onDismiss();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onDismiss]);

  const save = async () => {
    const connection = await useRemoteConnectionEditorStore.getState().save(port);
    if (!connection) return;
    onSaved(connection);
    onDismiss();
  };

  const view = {
    state,
    update: (patch: Parameters<typeof state.updateDraft>[0]) => state.updateDraft(patch),
    pick: (kind: Parameters<typeof state.pickCredentialFile>[1]) => void state.pickCredentialFile(port, kind),
  };

  return (
    <div className="remote-sheet-backdrop" role="presentation">
      <section aria-labelledby="remote-editor-title" aria-modal="true" className="remote-editor" role="dialog">
        <header className="remote-editor__header">
          <span aria-hidden="true">⌁</span>
          <div>
            <h2 id="remote-editor-title">{editingConnection ? "编辑远端连接" : "新建远端连接"}</h2>
            <p>SSH 由 ChatOS 客户端直接执行，凭据只保存在这台 Windows 设备。</p>
          </div>
        </header>
        <RemoteConnectionForm view={view} />
        <footer className="remote-editor__footer">
          <button disabled={busy} onClick={() => void state.testConnection(port)} type="button">
            {state.activeOperation?.kind === "test" ? <><span className="mini-spinner" />正在测试…</> : "测试连接"}
          </button>
          <span />
          <button disabled={busy} onClick={onDismiss} type="button">取消</button>
          <button className="primary-button" disabled={busy} onClick={() => void save()} type="button">
            {state.activeOperation?.kind === "save" ? <><span className="button-spinner" />正在保存…</> : editingConnection ? "保存" : "创建"}
          </button>
        </footer>
      </section>
      {state.verificationPrompt && (
        <RemoteVerificationDialog
          busy={state.activeOperation?.kind === "test"}
          code={state.verificationCode}
          errorMessage={state.errorMessage}
          prompt={state.verificationPrompt}
          onCancel={state.dismissVerification}
          onCodeChange={state.setVerificationCode}
          onSubmit={() => void state.testConnection(port, state.verificationCode)}
        />
      )}
    </div>
  );
}
