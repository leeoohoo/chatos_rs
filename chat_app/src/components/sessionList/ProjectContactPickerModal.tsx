import React from 'react';

interface ContactOption {
  id: string;
  name: string;
  agentId: string;
}

interface ProjectContactPickerModalProps {
  isOpen: boolean;
  projectName: string;
  contacts: ContactOption[];
  disabledContactIds?: string[];
  selectedContactId: string | null;
  error: string | null;
  onClose: () => void;
  onSelectedContactChange: (contactId: string) => void;
  onConfirm: () => void;
}

export const ProjectContactPickerModal: React.FC<ProjectContactPickerModalProps> = ({
  isOpen,
  projectName,
  contacts,
  disabledContactIds = [],
  selectedContactId,
  error,
  onClose,
  onSelectedContactChange,
  onConfirm,
}) => {
  if (!isOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-[520px] p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold text-foreground">添加联系人到项目</h3>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          >
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="text-sm text-muted-foreground mb-3">
          目标项目: <span className="text-foreground font-medium">{projectName || '未命名项目'}</span>
        </div>

        {contacts.length === 0 ? (
          <div className="text-sm text-muted-foreground">
            暂无可用联系人，请先在 CONTACTS 中添加联系人。
          </div>
        ) : (
          <div className="max-h-72 overflow-y-auto border border-border rounded">
            {contacts.map((contact) => {
              const selected = selectedContactId === contact.id;
              const disabled = disabledContactIds.includes(contact.id);
              return (
                <button
                  key={contact.id}
                  type="button"
                  onClick={() => {
                    if (disabled) {
                      return;
                    }
                    onSelectedContactChange(contact.id);
                  }}
                  disabled={disabled}
                  className={[
                    'w-full text-left px-3 py-2 border-b border-border last:border-b-0',
                    selected ? 'bg-accent' : 'hover:bg-accent/60',
                    disabled ? 'opacity-50 cursor-not-allowed hover:bg-transparent' : '',
                  ].join(' ')}
                >
                  <div className="flex items-center justify-between gap-2">
                    <div className="text-sm font-medium text-foreground truncate">{contact.name}</div>
                    {disabled ? (
                      <span className="text-[11px] px-1.5 py-0.5 rounded border border-border text-muted-foreground">
                        已添加
                      </span>
                    ) : null}
                  </div>
                  <div className="text-xs text-muted-foreground mt-1 truncate">{contact.agentId}</div>
                </button>
              );
            })}
          </div>
        )}

        {error ? (
          <div className="text-xs text-destructive mt-3">{error}</div>
        ) : null}

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
          >
            取消
          </button>
          <button
            onClick={onConfirm}
            disabled={!selectedContactId || contacts.length === 0 || disabledContactIds.includes(selectedContactId)}
            className="px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            添加到项目
          </button>
        </div>
      </div>
    </div>
  );
};
