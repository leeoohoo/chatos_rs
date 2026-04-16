import React from 'react';

interface RemoteVerificationModalProps {
  isOpen: boolean;
  prompt?: string | null;
  code: string;
  submitting?: boolean;
  onCodeChange: (value: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}

const RemoteVerificationModal: React.FC<RemoteVerificationModalProps> = ({
  isOpen,
  prompt,
  code,
  submitting = false,
  onCodeChange,
  onClose,
  onSubmit,
}) => {
  if (!isOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-[70] flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={submitting ? undefined : onClose} />
      <div className="relative w-[520px] max-w-[92vw] rounded-xl border border-border bg-card p-6 shadow-2xl">
        <h3 className="text-xl font-semibold text-foreground">Two-Step Verification required</h3>
        <p className="mt-2 text-sm text-muted-foreground">
          {prompt?.trim() || 'Please input verification code (SMS / OTP)'}
        </p>

        <input
          autoFocus
          type="text"
          value={code}
          onChange={(event) => onCodeChange(event.target.value)}
          className="mt-4 w-full rounded border border-border bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          placeholder="请输入验证码"
          disabled={submitting}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              onSubmit();
            }
          }}
        />

        <div className="mt-6 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            className="rounded bg-muted px-4 py-2 text-muted-foreground hover:bg-accent disabled:opacity-50"
          >
            Close
          </button>
          <button
            type="button"
            onClick={onSubmit}
            disabled={submitting || !code.trim()}
            className="rounded bg-primary px-4 py-2 text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {submitting ? 'Continue...' : 'Continue'}
          </button>
        </div>
      </div>
    </div>
  );
};

export default RemoteVerificationModal;
