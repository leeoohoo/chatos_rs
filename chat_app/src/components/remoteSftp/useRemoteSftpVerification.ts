import { useCallback, useRef, useState } from 'react';

import {
  extractSecondFactorPrompt as extractSecondFactorPromptFromError,
  isSecondFactorRequiredError,
  resolveRemoteSftpErrorMessage,
} from '../../lib/api/remoteConnectionErrors';

interface UseRemoteSftpVerificationOptions {
  setError: (message: string | null) => void;
  setMessage: (message: string | null) => void;
}

export const useRemoteSftpVerification = ({
  setError,
  setMessage,
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
    return extractSecondFactorPromptFromError(err, '请输入短信验证码或 OTP');
  }, []);

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
      setError('请输入验证码');
      return;
    }
    const pendingAction = pendingVerificationActionRef.current;
    if (!pendingAction) {
      setVerificationOpen(false);
      setError('验证码上下文已失效，请重试当前操作');
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
        setError('验证码错误或已过期，请重试');
        return;
      }
      setVerificationOpen(false);
      setVerificationPrompt('');
      setVerificationCodeInput('');
      pendingVerificationActionRef.current = null;
      setError(resolveRemoteSftpErrorMessage(err, 'SFTP 操作失败'));
    } finally {
      setVerificationSubmitting(false);
    }
  }, [
    extractSecondFactorPrompt,
    isSecondFactorRequired,
    setError,
    setMessage,
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
