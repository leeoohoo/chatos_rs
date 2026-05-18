import { useCallback, useRef, useState } from 'react';

import type { TranslateFn } from '../../i18n/I18nProvider';
import {
  extractSecondFactorPrompt as extractSecondFactorPromptFromError,
  isSecondFactorRequiredError,
  resolveRemoteSftpErrorMessage,
} from '../../lib/api/remoteConnectionErrors';

interface UseRemoteSftpVerificationOptions {
  setError: (message: string | null) => void;
  setMessage: (message: string | null) => void;
  t: TranslateFn;
}

export const useRemoteSftpVerification = ({
  setError,
  setMessage,
  t,
}: UseRemoteSftpVerificationOptions) => {
  const [activeVerificationCode, setActiveVerificationCode] = useState<string | null>(null);
  const [verificationOpen, setVerificationOpen] = useState(false);
  const [verificationPrompt, setVerificationPrompt] = useState('');
  const [verificationCodeInput, setVerificationCodeInput] = useState('');
  const [verificationSubmitting, setVerificationSubmitting] = useState(false);
  const pendingVerificationActionRef = useRef<((code: string) => Promise<void>) | null>(null);

  const isSecondFactorRequired = useCallback((err: unknown) => (
    isSecondFactorRequiredError(err)
  ), []);

  const extractSecondFactorPrompt = useCallback((err: unknown) => {
    return extractSecondFactorPromptFromError(err, t('remote.common.verificationPrompt'));
  }, [t]);

  const handleSecondFactorRequired = useCallback((
    err: unknown,
    retryWithCode: (code: string) => Promise<void>,
  ) => {
    if (!isSecondFactorRequired(err)) {
      return false;
    }
    pendingVerificationActionRef.current = retryWithCode;
    setVerificationPrompt(extractSecondFactorPrompt(err));
    setVerificationCodeInput('');
    setVerificationOpen(true);
    setVerificationSubmitting(false);
    setActiveVerificationCode(null);
    setError(null);
    setMessage(null);
    return true;
  }, [extractSecondFactorPrompt, isSecondFactorRequired, setError, setMessage]);

  const getVerificationCode = useCallback(() => activeVerificationCode, [activeVerificationCode]);

  const resetVerificationState = useCallback(() => {
    setActiveVerificationCode(null);
    setVerificationOpen(false);
    setVerificationPrompt('');
    setVerificationCodeInput('');
    setVerificationSubmitting(false);
    pendingVerificationActionRef.current = null;
  }, []);

  const closeVerification = useCallback(() => {
    if (verificationSubmitting) {
      return;
    }
    setVerificationOpen(false);
    setVerificationPrompt('');
    setVerificationCodeInput('');
    pendingVerificationActionRef.current = null;
  }, [verificationSubmitting]);

  const submitVerification = useCallback(async () => {
    const code = verificationCodeInput.trim();
    if (!code) {
      setError(t('remote.common.enterVerificationCode'));
      return;
    }
    const pendingAction = pendingVerificationActionRef.current;
    if (!pendingAction) {
      setVerificationOpen(false);
      setError(t('remote.common.verificationExpired'));
      return;
    }

    setVerificationSubmitting(true);
    setError(null);
    setMessage(null);
    try {
      await pendingAction(code);
      setActiveVerificationCode(code);
      setVerificationOpen(false);
      setVerificationPrompt('');
      setVerificationCodeInput('');
      pendingVerificationActionRef.current = null;
    } catch (err) {
      if (isSecondFactorRequired(err)) {
        setActiveVerificationCode(null);
        setVerificationPrompt(extractSecondFactorPrompt(err));
        setVerificationCodeInput('');
        setVerificationOpen(true);
        setError(t('remote.common.verificationInvalid'));
        return;
      }
      setVerificationOpen(false);
      setVerificationPrompt('');
      setVerificationCodeInput('');
      pendingVerificationActionRef.current = null;
      setError(resolveRemoteSftpErrorMessage(err, t('remote.sftp.error.operationFailed')));
    } finally {
      setVerificationSubmitting(false);
    }
  }, [
    extractSecondFactorPrompt,
    isSecondFactorRequired,
    setError,
    setMessage,
    t,
    verificationCodeInput,
  ]);

  return {
    activeVerificationCode,
    verificationOpen,
    verificationPrompt,
    verificationCodeInput,
    verificationSubmitting,
    setVerificationCodeInput,
    getVerificationCode,
    handleSecondFactorRequired,
    resetVerificationState,
    closeVerification,
    submitVerification,
  };
};
