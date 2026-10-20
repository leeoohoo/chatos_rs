import type { ReactNode } from "react";

export function RemoteFileTextDialog({
  busy,
  confirmLabel,
  description,
  title,
  value,
  onCancel,
  onConfirm,
  onValueChange,
}: {
  busy: boolean;
  confirmLabel: string;
  description: string;
  title: string;
  value: string;
  onCancel(): void;
  onConfirm(): void;
  onValueChange(value: string): void;
}) {
  return (
    <RemoteFileDialog title={title} description={description} onCancel={onCancel} actions={(
      <button className="primary-button" disabled={busy || !value.trim()} onClick={onConfirm} type="button">{confirmLabel}</button>
    )}>
      <label className="remote-file-dialog__field"><span>名称</span><input autoFocus disabled={busy} maxLength={1_024} onChange={(event) => onValueChange(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && value.trim() && !busy) onConfirm(); }} value={value} /></label>
    </RemoteFileDialog>
  );
}

export function RemoteFileConfirmDialog({
  busy,
  confirmLabel,
  danger = false,
  description,
  title,
  onCancel,
  onConfirm,
}: {
  busy: boolean;
  confirmLabel: string;
  danger?: boolean;
  description: string;
  title: string;
  onCancel(): void;
  onConfirm(): void;
}) {
  return (
    <RemoteFileDialog title={title} description={description} danger={danger} onCancel={onCancel} actions={(
      <button className={danger ? "danger-button" : "primary-button"} disabled={busy} onClick={onConfirm} type="button">{confirmLabel}</button>
    )} />
  );
}

export function RemoteFileVerificationDialog({
  busy,
  code,
  errorMessage,
  prompt,
  onCancel,
  onCodeChange,
  onSubmit,
}: {
  busy: boolean;
  code: string;
  errorMessage: string | null;
  prompt: string;
  onCancel(): void;
  onCodeChange(value: string): void;
  onSubmit(): void;
}) {
  return (
    <RemoteFileDialog title="SSH 二次验证" description={prompt || "请输入服务器要求的验证码。"} onCancel={onCancel} actions={(
      <button className="primary-button" disabled={busy || !code.trim()} onClick={onSubmit} type="button">验证</button>
    )}>
      <label className="remote-file-dialog__field"><span>验证码</span><input autoFocus disabled={busy} maxLength={1_024} onChange={(event) => onCodeChange(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && code.trim() && !busy) onSubmit(); }} value={code} /></label>
      {errorMessage && <p className="remote-file-dialog__error" role="alert">{errorMessage}</p>}
    </RemoteFileDialog>
  );
}

function RemoteFileDialog({
  actions,
  children,
  danger = false,
  description,
  title,
  onCancel,
}: {
  actions: ReactNode;
  children?: ReactNode;
  danger?: boolean;
  description: string;
  title: string;
  onCancel(): void;
}) {
  return (
    <div className="remote-file-dialog-backdrop" role="presentation">
      <section aria-labelledby="remote-file-dialog-title" aria-modal="true" className="remote-file-dialog" role={danger ? "alertdialog" : "dialog"}>
        <span className={danger ? "is-danger" : ""} aria-hidden="true">{danger ? "!" : "▰"}</span>
        <h3 id="remote-file-dialog-title">{title}</h3>
        <p>{description}</p>
        {children}
        <footer><button onClick={onCancel} type="button">取消</button>{actions}</footer>
      </section>
    </div>
  );
}
