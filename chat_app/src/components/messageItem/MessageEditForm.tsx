import type { FC } from 'react';

interface MessageEditFormProps {
  editContent: string;
  onEditContentChange: (value: string) => void;
  onSave: () => void;
  onCancel: () => void;
}

export const MessageEditForm: FC<MessageEditFormProps> = ({
  editContent,
  onEditContentChange,
  onSave,
  onCancel,
}) => (
  <div className="space-y-2">
    <textarea
      value={editContent}
      onChange={(e) => onEditContentChange(e.target.value)}
      className="w-full p-2 border rounded-md resize-none focus:outline-none focus:ring-2 focus:ring-primary"
      rows={3}
      autoFocus
    />
    <div className="flex gap-2">
      <button
        onClick={onSave}
        className="px-3 py-1 text-sm bg-primary text-primary-foreground rounded hover:bg-primary/90"
      >
        Save
      </button>
      <button
        onClick={onCancel}
        className="px-3 py-1 text-sm bg-muted text-muted-foreground rounded hover:bg-muted/80"
      >
        Cancel
      </button>
    </div>
  </div>
);
