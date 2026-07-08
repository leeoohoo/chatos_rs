// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

interface ManagerFormDialogProps {
  open: boolean;
  title: string;
  description?: string;
  widthClassName?: string;
  bodyClassName?: string;
  onClose: () => void;
  children: React.ReactNode;
}

const CloseIcon = () => (
  <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
  </svg>
);

const ManagerFormDialog: React.FC<ManagerFormDialogProps> = ({
  open,
  title,
  description,
  widthClassName = 'max-w-2xl',
  bodyClassName = 'p-6',
  onClose,
  children,
}) => {
  const titleId = React.useId();

  React.useEffect(() => {
    if (!open) {
      return undefined;
    }

    const previousOverflow = document.body.style.overflow;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };

    document.body.style.overflow = 'hidden';
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [open, onClose]);

  if (!open) {
    return null;
  }

  return (
    <div
      className="fixed inset-0 z-[70] flex items-start justify-center overflow-y-auto bg-black/55 p-4 pt-10 backdrop-blur-sm sm:items-center sm:pt-4"
      onClick={onClose}
      role="presentation"
    >
      <div
        className={`flex max-h-[88vh] w-full ${widthClassName} flex-col overflow-hidden rounded-2xl border border-border bg-card shadow-2xl`}
        onClick={(event) => event.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <div className="flex items-start justify-between gap-4 border-b border-border px-6 py-4">
          <div className="min-w-0">
            <h3 id={titleId} className="text-lg font-semibold text-foreground">{title}</h3>
            {description ? (
              <p className="mt-1 text-sm text-muted-foreground">{description}</p>
            ) : null}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-2 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            aria-label="Close"
          >
            <CloseIcon />
          </button>
        </div>

        <div className={`overflow-y-auto ${bodyClassName}`}>
          {children}
        </div>
      </div>
    </div>
  );
};

export default ManagerFormDialog;
