import type { RemoteConnectionEditorStateView } from "./remoteConnectionEditorView";

export function RemoteConnectionForm({ view }: { view: RemoteConnectionEditorStateView }) {
  const { state } = view;
  const draft = state.draft;
  const busy = state.activeOperation != null;
  const availableJumpConnections = state.connections.filter(
    (connection) => connection.id !== state.editingConnection?.id,
  );

  return (
    <div className="remote-editor__body">
      <fieldset className="remote-form-section" disabled={busy}>
        <legend>连接</legend>
        <label className="remote-form-field">
          <span>名称 <small>可选</small></span>
          <input autoFocus maxLength={1_000} onChange={(event) => view.update({ name: event.target.value })} placeholder="例如：生产服务器" value={draft.name} />
        </label>
        <div className="remote-form-columns remote-form-columns--host">
          <label className="remote-form-field">
            <span>主机</span>
            <input maxLength={1_000} onChange={(event) => view.update({ host: event.target.value })} placeholder="server.example.com" value={draft.host} />
          </label>
          <label className="remote-form-field">
            <span>端口</span>
            <input inputMode="numeric" onChange={(event) => view.update({ port: event.target.value })} value={draft.port} />
          </label>
        </div>
        <label className="remote-form-field">
          <span>用户名</span>
          <input maxLength={1_000} onChange={(event) => view.update({ username: event.target.value })} value={draft.username} />
        </label>
        <label className="remote-form-field">
          <span>主机密钥</span>
          <select onChange={(event) => view.update({ hostKeyPolicy: event.target.value as typeof draft.hostKeyPolicy })} value={draft.hostKeyPolicy}>
            <option value="strict">严格校验 known_hosts</option>
            <option value="accept_new">首次连接时接受新密钥</option>
          </select>
        </label>
      </fieldset>

      <fieldset className="remote-form-section" disabled={busy}>
        <legend>认证</legend>
        <label className="remote-form-field">
          <span>认证方式</span>
          <select onChange={(event) => view.update({ authenticationType: event.target.value as typeof draft.authenticationType })} value={draft.authenticationType}>
            <option value="private_key">私钥</option>
            <option value="private_key_cert">私钥 + 证书</option>
            <option value="password">密码</option>
          </select>
        </label>
        {draft.authenticationType === "password" ? (
          <label className="remote-form-field">
            <span>登录密码</span>
            <input
              autoComplete="new-password"
              onChange={(event) => view.update({ password: event.target.value })}
              placeholder={state.editingConnection?.hasPassword ? "留空则继续使用已保存密码" : "输入登录密码"}
              type="password"
              value={draft.password}
            />
          </label>
        ) : (
          <>
            <CredentialFileRow
              label="私钥文件"
              saved={state.editingConnection?.hasPrivateKeyPath === true}
              selectionName={draft.privateKeySelection?.fileName ?? null}
              onPick={() => view.pick("private_key")}
            />
            {draft.authenticationType === "private_key_cert" && (
              <CredentialFileRow
                label="SSH 证书文件"
                saved={state.editingConnection?.hasCertificatePath === true}
                selectionName={draft.certificateSelection?.fileName ?? null}
                onPick={() => view.pick("certificate")}
              />
            )}
          </>
        )}
        <label className="remote-form-field">
          <span>默认远端目录 <small>可选</small></span>
          <input maxLength={4_000} onChange={(event) => view.update({ defaultRemotePath: event.target.value })} placeholder="例如 /srv/app；留空使用登录目录" value={draft.defaultRemotePath} />
        </label>
      </fieldset>

      <fieldset className="remote-form-section" disabled={busy}>
        <legend>跳板机</legend>
        <label className="remote-toggle-row">
          <span><strong>通过跳板机连接</strong><small>复用已有连接，或为这条连接单独配置。</small></span>
          <input checked={draft.jumpEnabled} onChange={(event) => view.update({ jumpEnabled: event.target.checked })} type="checkbox" />
        </label>
        {draft.jumpEnabled && (
          <>
            <div aria-label="跳板机来源" className="remote-segmented" role="group">
              <button className={draft.jumpMode === "existing" ? "is-selected" : ""} disabled={availableJumpConnections.length === 0} onClick={() => view.update({ jumpMode: "existing" })} type="button">已有连接</button>
              <button className={draft.jumpMode === "manual" ? "is-selected" : ""} onClick={() => view.update({ jumpMode: "manual" })} type="button">手动配置</button>
            </div>
            {draft.jumpMode === "existing" && availableJumpConnections.length > 0 ? (
              <label className="remote-form-field">
                <span>已有连接</span>
                <select onChange={(event) => view.update({ jumpConnectionId: event.target.value })} value={draft.jumpConnectionId}>
                  <option value="">请选择</option>
                  {availableJumpConnections.map((connection) => (
                    <option key={connection.id} value={connection.id}>{connection.name} — {connection.username}@{connection.host}:{connection.port}</option>
                  ))}
                </select>
              </label>
            ) : (
              <ManualJumpFields view={view} />
            )}
          </>
        )}
      </fieldset>

      <p className="remote-local-note"><span aria-hidden="true">⌘</span>凭据保存在 Windows 凭据管理器；React 页面不会读取密码或本机文件路径。</p>
      {state.successMessage && <p className="remote-feedback remote-feedback--success" role="status">✓ {state.successMessage}</p>}
      {state.errorMessage && !state.verificationPrompt && <p className="remote-feedback remote-feedback--error" role="alert">! {state.errorMessage}</p>}
    </div>
  );
}

function ManualJumpFields({ view }: { view: RemoteConnectionEditorStateView }) {
  const { draft } = view.state;
  return (
    <div className="remote-form-stack">
      <div className="remote-form-columns remote-form-columns--host">
        <label className="remote-form-field"><span>跳板机地址</span><input onChange={(event) => view.update({ jumpHost: event.target.value })} value={draft.jumpHost} /></label>
        <label className="remote-form-field"><span>端口</span><input inputMode="numeric" onChange={(event) => view.update({ jumpPort: event.target.value })} value={draft.jumpPort} /></label>
      </div>
      <label className="remote-form-field"><span>跳板机用户名</span><input onChange={(event) => view.update({ jumpUsername: event.target.value })} value={draft.jumpUsername} /></label>
      <CredentialFileRow label="跳板机私钥（可选）" saved={view.state.editingConnection?.hasJumpPrivateKeyPath === true} selectionName={draft.jumpPrivateKeySelection?.fileName ?? null} onPick={() => view.pick("jump_private_key")} />
      <CredentialFileRow label="跳板机证书（可选）" saved={view.state.editingConnection?.hasJumpCertificatePath === true} selectionName={draft.jumpCertificateSelection?.fileName ?? null} onPick={() => view.pick("jump_certificate")} />
      <label className="remote-form-field"><span>跳板机密码 <small>可选</small></span><input autoComplete="new-password" onChange={(event) => view.update({ jumpPassword: event.target.value })} placeholder={view.state.editingConnection?.hasJumpPassword ? "留空则继续使用已保存密码" : "可与私钥配合使用"} type="password" value={draft.jumpPassword} /></label>
    </div>
  );
}

function CredentialFileRow({ label, saved, selectionName, onPick }: { label: string; saved: boolean; selectionName: string | null; onPick(): void }) {
  return (
    <div className="remote-file-row">
      <span><strong>{label}</strong><small>{selectionName ?? (saved ? "继续使用已保存文件" : "尚未选择")}</small></span>
      <button onClick={onPick} type="button">选择…</button>
    </div>
  );
}
