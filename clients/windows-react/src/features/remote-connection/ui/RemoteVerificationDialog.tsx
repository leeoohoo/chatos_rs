export function RemoteVerificationDialog({
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
    <div className="remote-dialog-backdrop" role="presentation">
      <section aria-labelledby="remote-verification-title" aria-modal="true" className="remote-dialog" role="dialog">
        <span className="remote-dialog__icon" aria-hidden="true">⌁</span>
        <h2 id="remote-verification-title">SSH 二次验证</h2>
        <p>{prompt || "请输入服务器要求的验证码。"}</p>
        <label className="remote-form-field remote-dialog__field">
          <span>验证码</span>
          <input
            autoFocus
            disabled={busy}
            maxLength={1_024}
            onChange={(event) => onCodeChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && code.trim() && !busy) onSubmit();
            }}
            value={code}
          />
        </label>
        {errorMessage && <p className="remote-feedback remote-feedback--error" role="alert">{errorMessage}</p>}
        <footer>
          <button disabled={busy} onClick={onCancel} type="button">取消</button>
          <button className="primary-button" disabled={busy || !code.trim()} onClick={onSubmit} type="button">
            {busy ? <><span className="button-spinner" />正在验证…</> : "验证"}
          </button>
        </footer>
      </section>
    </div>
  );
}
