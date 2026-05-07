import React from 'react';

export const GitActionRows: React.FC<{
  actionLoading: boolean;
  onFetch: () => Promise<void>;
  onPull: () => Promise<void>;
  onPush: () => Promise<void>;
  onOpenCommit: () => void;
}> = ({ actionLoading, onFetch, onPull, onPush, onOpenCommit }) => {
  const actions = [
    { label: 'Fetch', run: onFetch },
    { label: 'Pull --ff-only', run: onPull },
    { label: 'Push', run: onPush },
  ];
  return (
    <div className="mb-2 grid grid-cols-2 gap-2">
      {actions.map((action) => (
        <button
          key={action.label}
          type="button"
          onClick={() => { void action.run(); }}
          disabled={actionLoading}
          className="h-8 rounded border border-border px-3 text-left text-xs hover:bg-accent disabled:opacity-50"
        >
          {action.label}
        </button>
      ))}
      <button
        type="button"
        onClick={onOpenCommit}
        disabled={actionLoading}
        className="h-8 rounded border border-border px-3 text-left text-xs hover:bg-accent disabled:opacity-50"
      >
        Commit...
      </button>
    </div>
  );
};

export const NewBranchRow: React.FC<{
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
  onCreate: () => Promise<void>;
}> = ({ value, disabled, onChange, onCreate }) => (
  <div className="mb-3 flex gap-2">
    <input
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder="New Branch..."
      className="h-8 min-w-0 flex-1 rounded border border-border bg-background px-2 text-xs outline-none focus:border-primary"
    />
    <button
      type="button"
      disabled={disabled || !value.trim()}
      onClick={() => { void onCreate(); }}
      className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50"
    >
      创建
    </button>
  </div>
);
