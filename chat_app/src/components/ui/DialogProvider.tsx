import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import AlertDialog from './AlertDialog';
import ConfirmDialog from './ConfirmDialog';
import PromptDialog from './PromptDialog';

export interface DialogAlertOptions {
  title: string;
  message: string;
  description?: string;
  confirmText?: string;
  type?: 'danger' | 'warning' | 'info';
}

export interface DialogConfirmOptions {
  title: string;
  message: string;
  description?: string;
  details?: string;
  detailsTitle?: string;
  detailsLines?: string[];
  confirmText?: string;
  cancelText?: string;
  type?: 'danger' | 'warning' | 'info';
}

export interface DialogPromptOptions {
  title: string;
  message: string;
  description?: string;
  inputLabel?: string;
  placeholder?: string;
  defaultValue?: string;
  confirmText?: string;
  cancelText?: string;
  type?: 'danger' | 'warning' | 'info';
  validate?: (value: string) => string | null;
}

interface DialogService {
  alert: (options: DialogAlertOptions) => Promise<void>;
  confirm: (options: DialogConfirmOptions) => Promise<boolean>;
  prompt: (options: DialogPromptOptions) => Promise<string | null>;
}

interface AlertDialogRuntimeState extends DialogAlertOptions {
  isOpen: boolean;
}

interface ConfirmDialogRuntimeState extends DialogConfirmOptions {
  isOpen: boolean;
}

interface PromptDialogRuntimeState extends DialogPromptOptions {
  isOpen: boolean;
  value: string;
  error: string | null;
}

const DialogServiceContext = createContext<DialogService | null>(null);

interface DialogProviderProps {
  children: React.ReactNode;
}

export const DialogProvider: React.FC<DialogProviderProps> = ({ children }) => {
  const alertResolverRef = useRef<(() => void) | null>(null);
  const confirmResolverRef = useRef<((value: boolean) => void) | null>(null);
  const promptResolverRef = useRef<((value: string | null) => void) | null>(null);
  const [alertState, setAlertState] = useState<AlertDialogRuntimeState | null>(null);
  const [confirmState, setConfirmState] = useState<ConfirmDialogRuntimeState | null>(null);
  const [promptState, setPromptState] = useState<PromptDialogRuntimeState | null>(null);

  const settleAlert = useCallback(() => {
    const resolver = alertResolverRef.current;
    alertResolverRef.current = null;
    setAlertState(null);
    resolver?.();
  }, []);

  const settleConfirm = useCallback((value: boolean) => {
    const resolver = confirmResolverRef.current;
    confirmResolverRef.current = null;
    setConfirmState(null);
    resolver?.(value);
  }, []);

  const settlePrompt = useCallback((value: string | null) => {
    const resolver = promptResolverRef.current;
    promptResolverRef.current = null;
    setPromptState(null);
    resolver?.(value);
  }, []);

  const closeActiveDialog = useCallback(() => {
    alertResolverRef.current?.();
    alertResolverRef.current = null;
    setAlertState(null);

    confirmResolverRef.current?.(false);
    confirmResolverRef.current = null;
    setConfirmState(null);

    promptResolverRef.current?.(null);
    promptResolverRef.current = null;
    setPromptState(null);
  }, []);

  const alert = useCallback((options: DialogAlertOptions) => {
    closeActiveDialog();
    setAlertState({
      isOpen: true,
      ...options,
    });
    return new Promise<void>((resolve) => {
      alertResolverRef.current = resolve;
    });
  }, [closeActiveDialog]);

  const confirm = useCallback((options: DialogConfirmOptions) => {
    closeActiveDialog();
    setConfirmState({
      isOpen: true,
      ...options,
    });
    return new Promise<boolean>((resolve) => {
      confirmResolverRef.current = resolve;
    });
  }, [closeActiveDialog]);

  const prompt = useCallback((options: DialogPromptOptions) => {
    closeActiveDialog();
    setPromptState({
      isOpen: true,
      value: options.defaultValue || '',
      error: null,
      ...options,
    });
    return new Promise<string | null>((resolve) => {
      promptResolverRef.current = resolve;
    });
  }, [closeActiveDialog]);

  useEffect(() => () => {
    closeActiveDialog();
  }, [closeActiveDialog]);

  const handlePromptConfirm = useCallback(() => {
    if (!promptState) {
      return;
    }
    const nextError = promptState.validate?.(promptState.value) || null;
    if (nextError) {
      setPromptState((prev) => (prev ? { ...prev, error: nextError } : prev));
      return;
    }
    settlePrompt(promptState.value);
  }, [promptState, settlePrompt]);

  const value = useMemo<DialogService>(() => ({
    alert,
    confirm,
    prompt,
  }), [alert, confirm, prompt]);

  return (
    <DialogServiceContext.Provider value={value}>
      {children}
      <AlertDialog
        isOpen={Boolean(alertState?.isOpen)}
        title={alertState?.title || ''}
        message={alertState?.message || ''}
        description={alertState?.description}
        confirmText={alertState?.confirmText}
        type={alertState?.type}
        onConfirm={settleAlert}
      />
      <ConfirmDialog
        isOpen={Boolean(confirmState?.isOpen)}
        title={confirmState?.title || ''}
        message={confirmState?.message || ''}
        description={confirmState?.description}
        details={confirmState?.details}
        detailsTitle={confirmState?.detailsTitle}
        detailsLines={confirmState?.detailsLines}
        confirmText={confirmState?.confirmText}
        cancelText={confirmState?.cancelText}
        type={confirmState?.type}
        onConfirm={() => settleConfirm(true)}
        onCancel={() => settleConfirm(false)}
      />
      <PromptDialog
        isOpen={Boolean(promptState?.isOpen)}
        title={promptState?.title || ''}
        message={promptState?.message || ''}
        description={promptState?.description}
        inputLabel={promptState?.inputLabel}
        placeholder={promptState?.placeholder}
        value={promptState?.value || ''}
        error={promptState?.error}
        confirmText={promptState?.confirmText}
        cancelText={promptState?.cancelText}
        type={promptState?.type}
        onValueChange={(nextValue) => {
          setPromptState((prev) => (prev ? {
            ...prev,
            value: nextValue,
            error: null,
          } : prev));
        }}
        onConfirm={handlePromptConfirm}
        onCancel={() => settlePrompt(null)}
      />
    </DialogServiceContext.Provider>
  );
};

export const useDialogService = (): DialogService => {
  const service = useContext(DialogServiceContext);
  if (!service) {
    throw new Error('useDialogService must be used within DialogProvider');
  }
  return service;
};
