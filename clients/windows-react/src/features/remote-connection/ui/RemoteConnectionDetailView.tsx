import { useEffect, useLayoutEffect, useState } from "react";
import { useRemoteConnectionDetailStore } from "../application/remoteConnectionDetailStore";
import type { RemoteConnection } from "../domain/remoteConnection";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";
import type { RemoteFilePort } from "../../remote-files/ports/RemoteFilePort";
import { RemoteSftpWorkspace } from "../../remote-files/ui/RemoteSftpWorkspace";
import { RemoteVerificationDialog } from "./RemoteVerificationDialog";
import "./remoteConnection.css";

type RemoteWorkspaceTab = "terminal" | "files" | "details";

const tabs: Array<{ id: RemoteWorkspaceTab; label: string; icon: string }> = [
  { id: "terminal", label: "远程终端", icon: ">_" },
  { id: "files", label: "SFTP 文件", icon: "▰" },
  { id: "details", label: "连接信息", icon: "ⓘ" },
];

export function RemoteConnectionDetailView({
  connection,
  connections,
  ownerUserId,
  port,
  remoteFilePort,
  onDeleted,
  onEdit,
}: {
  connection: RemoteConnection;
  connections: RemoteConnection[];
  ownerUserId: string;
  port: RemoteConnectionPort;
  remoteFilePort: RemoteFilePort;
  onDeleted(connectionId: string): void;
  onEdit(): void;
}) {
  const state = useRemoteConnectionDetailStore();
  const [selectedTab, setSelectedTab] = useState<RemoteWorkspaceTab>("details");
  const [showDeleteConfirmation, setShowDeleteConfirmation] = useState(false);
  const busy = state.activeOperation != null;

  useLayoutEffect(() => {
    useRemoteConnectionDetailStore.getState().activate(port, ownerUserId, connection);
    return () => useRemoteConnectionDetailStore.getState().deactivate(port);
  }, [connection.id, ownerUserId, port]);

  useEffect(() => {
    useRemoteConnectionDetailStore.getState().updateConnection(ownerUserId, connection);
  }, [connection, ownerUserId]);

  useEffect(() => {
    setSelectedTab("details");
    setShowDeleteConfirmation(false);
  }, [connection.id]);

  const remove = async () => {
    const deletedConnectionId = await useRemoteConnectionDetailStore.getState().deleteConnection(port);
    if (!deletedConnectionId) return;
    useRemoteConnectionDetailStore.getState().consumeDeletedConnection();
    onDeleted(deletedConnectionId);
  };

  return (
    <section className="remote-workspace" aria-label={`${connection.name} 远端工作区`}>
      <header className="remote-workspace__header">
        <div className="remote-workspace__identity">
          <span aria-hidden="true">⌁</span>
          <div><h2>{connection.name}</h2><p>{connection.username}@{connection.host}:{connection.port}</p></div>
        </div>
        <div className="remote-workspace__actions">
          <span className={`remote-status ${state.activeOperation?.kind === "test" ? "remote-status--testing" : ""}`}>
            {state.activeOperation?.kind === "test" ? "测试中" : "已保存"}
          </span>
          <button disabled={busy} onClick={() => void state.testSavedConnection(port)} type="button">测试连接</button>
          <button disabled={busy} onClick={onEdit} type="button">编辑</button>
          <button aria-label="删除连接" className="remote-danger-link" disabled={busy} onClick={() => setShowDeleteConfirmation(true)} type="button">删除</button>
        </div>
        {(state.notice || state.errorMessage) && (
          <p className={`remote-workspace__feedback ${state.errorMessage ? "is-error" : ""}`} role={state.errorMessage ? "alert" : "status"}>
            {state.errorMessage ?? state.notice}
          </p>
        )}
        <div aria-label="远端工作区" className="remote-workspace-tabs" role="tablist">
          {tabs.map((tab) => (
            <button
              aria-selected={selectedTab === tab.id}
              className={selectedTab === tab.id ? "is-selected" : ""}
              key={tab.id}
              onClick={() => setSelectedTab(tab.id)}
              role="tab"
              type="button"
            ><span aria-hidden="true">{tab.icon}</span>{tab.label}</button>
          ))}
        </div>
      </header>
      <div className={`remote-workspace__content ${selectedTab === "files" ? "remote-workspace__content--files" : ""}`} role="tabpanel">
        {selectedTab === "details" ? (
          <ConnectionDetails connection={connection} connections={connections} onOpenFiles={() => setSelectedTab("files")} onOpenTerminal={() => setSelectedTab("terminal")} />
        ) : selectedTab === "files" ? (
          <RemoteSftpWorkspace connectionId={connection.id} ownerUserId={ownerUserId} port={remoteFilePort} />
        ) : (
          <PendingWorkspace connection={connection} />
        )}
      </div>

      {state.verificationPrompt && (
        <RemoteVerificationDialog
          busy={state.activeOperation?.kind === "test"}
          code={state.verificationCode}
          errorMessage={state.errorMessage}
          prompt={state.verificationPrompt}
          onCancel={state.dismissVerification}
          onCodeChange={state.setVerificationCode}
          onSubmit={() => void state.testSavedConnection(port, state.verificationCode)}
        />
      )}
      {showDeleteConfirmation && (
        <div className="remote-dialog-backdrop" role="presentation">
          <section aria-labelledby="remote-delete-title" aria-modal="true" className="remote-dialog" role="alertdialog">
            <span className="remote-dialog__icon remote-dialog__icon--danger" aria-hidden="true">!</span>
            <h2 id="remote-delete-title">删除远端连接？</h2>
            <p>只会删除 ChatOS 保存的连接配置，不会修改远端服务器。</p>
            {state.errorMessage && <p className="remote-feedback remote-feedback--error" role="alert">{state.errorMessage}</p>}
            <footer>
              <button disabled={busy} onClick={() => setShowDeleteConfirmation(false)} type="button">取消</button>
              <button className="danger-button" disabled={busy} onClick={() => void remove()} type="button">
                {state.activeOperation?.kind === "delete" ? "正在删除…" : "删除"}
              </button>
            </footer>
          </section>
        </div>
      )}
    </section>
  );
}

function ConnectionDetails({ connection, connections, onOpenFiles, onOpenTerminal }: { connection: RemoteConnection; connections: RemoteConnection[]; onOpenFiles(): void; onOpenTerminal(): void }) {
  const jump = connection.jumpConnectionId
    ? connections.find((candidate) => candidate.id === connection.jumpConnectionId)
    : null;
  const jumpLabel = !connection.jumpEnabled
    ? "未启用"
    : jump
      ? `${jump.name}（${jump.username}@${jump.host}）`
      : connection.jumpHost ?? "已启用";
  return (
    <article className="remote-detail-card">
      <div className="remote-detail-card__buttons">
        <button className="primary-button" onClick={onOpenTerminal} type="button">连接终端</button>
        <button onClick={onOpenFiles} type="button">打开 SFTP</button>
      </div>
      <dl>
        <DetailRow label="认证方式" value={authenticationLabel(connection.authenticationType)} />
        <DetailRow label="主机密钥" value={connection.hostKeyPolicy === "strict" ? "严格校验" : "首次接受新密钥"} />
        <DetailRow label="默认目录" value={connection.defaultRemotePath ?? "登录目录"} />
        <DetailRow label="执行位置" value="这台 Windows 设备 · ChatOS 客户端" />
        <DetailRow label="跳板机" value={jumpLabel} />
      </dl>
    </article>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd>{value}</dd></div>;
}

function PendingWorkspace({ connection }: { connection: RemoteConnection }) {
  return (
    <div className="remote-workspace-placeholder">
      <span aria-hidden="true">{">_"}</span>
      <h3>远程终端</h3>
      <p>{connection.username}@{connection.host} 的交互式终端将在连接建立后显示在这里。</p>
      <small>原生安全传输层已就绪，界面连接功能正在接入。</small>
    </div>
  );
}

function authenticationLabel(type: RemoteConnection["authenticationType"]) {
  if (type === "password") return "密码";
  if (type === "private_key_cert") return "私钥 + 证书";
  return "私钥";
}
