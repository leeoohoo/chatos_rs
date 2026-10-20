import { create } from "zustand";
import type {
  AuthSession,
  AuthenticationMode,
  AuthenticationPhase,
  LoginSubmission,
  RegistrationCodeSubmission,
  RegistrationSubmission,
} from "../domain/authentication";
import type { AuthenticationPort } from "../ports/AuthenticationPort";

interface AuthenticationState {
  phase: AuthenticationPhase;
  mode: AuthenticationMode;
  session: AuthSession | null;
  errorMessage: string | null;
  registrationMessage: string | null;
  isSendingRegistrationCode: boolean;
  registrationCodeCountdown: number;
  shouldRetrySessionRestore: boolean;
  operationId: number;
  codeOperationId: number;
  didStart: boolean;
  start(port: AuthenticationPort): Promise<void>;
  retrySessionRestoreIfNeeded(port: AuthenticationPort): Promise<void>;
  login(port: AuthenticationPort, submission: LoginSubmission): Promise<void>;
  sendRegistrationCode(
    port: AuthenticationPort,
    submission: RegistrationCodeSubmission,
  ): Promise<void>;
  register(port: AuthenticationPort, submission: RegistrationSubmission): Promise<void>;
  logout(
    port: AuthenticationPort,
    beforeCredentialClear?: () => Promise<void>,
  ): Promise<void>;
  showRegistration(): void;
  showLogin(): void;
}

let activeAuthentication: Promise<void> | null = null;
let pendingLogout: Promise<void> | null = null;
let countdownTimer: ReturnType<typeof setInterval> | null = null;

export const useAuthenticationStore = create<AuthenticationState>((set, get) => ({
  phase: "restoring",
  mode: "signIn",
  session: null,
  errorMessage: null,
  registrationMessage: null,
  isSendingRegistrationCode: false,
  registrationCodeCountdown: 0,
  shouldRetrySessionRestore: false,
  operationId: 0,
  codeOperationId: 0,
  didStart: false,

  async start(port) {
    if (get().didStart) return;
    set({ didStart: true });
    await restoreSession(port, set, get);
  },

  async retrySessionRestoreIfNeeded(port) {
    if (!get().shouldRetrySessionRestore || get().phase !== "signedOut") return;
    await restoreSession(port, set, get);
  },

  async login(port, submission) {
    await authenticate(
      port,
      () => port.login({ username: submission.username, password: submission.password }),
      set,
      get,
    );
  },

  async sendRegistrationCode(port, submission) {
    const error = registrationValidation(submission.email, submission.inviteCode);
    if (error) {
      set({ errorMessage: error });
      return;
    }
    const codeOperationId = get().codeOperationId + 1;
    set({
      codeOperationId,
      isSendingRegistrationCode: true,
      errorMessage: null,
      registrationMessage: null,
    });
    try {
      const delivery = await port.sendRegistrationCode(submission);
      if (get().codeOperationId !== codeOperationId) return;
      set({ registrationMessage: "验证码已发送，请查看邮箱。" });
      startCountdown(delivery.resendAfterSeconds, set, get);
    } catch (error) {
      if (get().codeOperationId !== codeOperationId) return;
      set({ errorMessage: friendlyRegistrationError(error) });
    } finally {
      if (get().codeOperationId === codeOperationId) {
        set({ isSendingRegistrationCode: false });
      }
    }
  },

  async register(port, submission) {
    const validation = validateRegistration(submission);
    if (validation) {
      set({ errorMessage: validation });
      return;
    }
    await authenticate(port, () => port.register(submission), set, get, true);
  },

  async logout(port, beforeCredentialClear) {
    const operationId = get().operationId + 1;
    const codeOperationId = get().codeOperationId + 1;
    const authentication = activeAuthentication;
    const previousLogout = pendingLogout;
    stopCountdown();
    set({
      operationId,
      codeOperationId,
      phase: "signedOut",
      mode: "signIn",
      session: null,
      errorMessage: null,
      registrationMessage: null,
      isSendingRegistrationCode: false,
      registrationCodeCountdown: 0,
      shouldRetrySessionRestore: false,
    });
    const logout = (async () => {
      await previousLogout?.catch(() => undefined);
      await authentication?.catch(() => undefined);
      await beforeCredentialClear?.();
      await port.logout();
    })();
    pendingLogout = logout;
    try {
      await logout;
    } finally {
      if (pendingLogout === logout) pendingLogout = null;
    }
  },

  showRegistration() {
    set({ mode: "register", errorMessage: null, registrationMessage: null });
  },

  showLogin() {
    const codeOperationId = get().codeOperationId + 1;
    stopCountdown();
    set({
      mode: "signIn",
      codeOperationId,
      errorMessage: null,
      registrationMessage: null,
      isSendingRegistrationCode: false,
      registrationCodeCountdown: 0,
    });
  },
}));

type SetState = (partial: Partial<AuthenticationState>) => void;
type GetState = () => AuthenticationState;

async function restoreSession(
  port: AuthenticationPort,
  set: SetState,
  get: GetState,
): Promise<void> {
  const operationId = get().operationId + 1;
  set({ phase: "restoring", errorMessage: null, operationId });
  await runAuthentication(async () => {
    try {
      await pendingLogout?.catch(() => undefined);
      const session = await port.restoreSession();
      if (get().operationId !== operationId) return;
      set({
        phase: session ? "authenticated" : "signedOut",
        session,
        shouldRetrySessionRestore: false,
      });
    } catch (error) {
      if (get().operationId !== operationId) return;
      set({
        phase: "signedOut",
        session: null,
        errorMessage: errorMessage(error),
        shouldRetrySessionRestore: true,
      });
    }
  });
}

async function authenticate(
  port: AuthenticationPort,
  operation: () => Promise<AuthSession>,
  set: SetState,
  get: GetState,
  registration = false,
): Promise<void> {
  const operationId = get().operationId + 1;
  set({
    phase: "authenticating",
    session: null,
    errorMessage: null,
    registrationMessage: null,
    shouldRetrySessionRestore: false,
    operationId,
  });
  await runAuthentication(async () => {
    try {
      await pendingLogout?.catch(() => undefined);
      const session = await operation();
      if (get().operationId !== operationId) return;
      if (registration) stopCountdown();
      set({
        phase: "authenticated",
        session,
        registrationCodeCountdown: registration ? 0 : get().registrationCodeCountdown,
      });
    } catch (error) {
      if (get().operationId !== operationId) return;
      set({
        phase: "signedOut",
        session: null,
        errorMessage: registration
          ? friendlyRegistrationError(error)
          : errorMessage(error),
      });
    }
  });
  void port;
}

async function runAuthentication(operation: () => Promise<void>): Promise<void> {
  const promise = operation();
  activeAuthentication = promise;
  try {
    await promise;
  } finally {
    if (activeAuthentication === promise) activeAuthentication = null;
  }
}

function startCountdown(seconds: number, set: SetState, get: GetState): void {
  stopCountdown();
  set({ registrationCodeCountdown: Math.max(1, seconds) });
  countdownTimer = setInterval(() => {
    const remaining = get().registrationCodeCountdown;
    if (remaining <= 1) {
      stopCountdown();
      set({ registrationCodeCountdown: 0 });
    } else {
      set({ registrationCodeCountdown: remaining - 1 });
    }
  }, 1_000);
}

function stopCountdown(): void {
  if (countdownTimer !== null) clearInterval(countdownTimer);
  countdownTimer = null;
}

function registrationValidation(email: string, inviteCode: string): string | null {
  if (!isLikelyEmail(email)) return "请输入有效的邮箱地址。";
  if (!inviteCode.trim()) return "请输入邀请码。";
  return null;
}

function validateRegistration(submission: RegistrationSubmission): string | null {
  const base = registrationValidation(submission.email, submission.inviteCode);
  if (base) return base;
  const code = submission.verificationCode.trim();
  if (!/^\d{6}$/.test(code)) return "请输入 6 位邮箱验证码。";
  if (submission.password.length < 12) return "密码至少需要 12 个字符。";
  if (submission.password.length > 128) return "密码最多允许 128 个字符。";
  return null;
}

function isLikelyEmail(value: string): boolean {
  const parts = value.trim().split("@");
  return parts.length === 2 && parts[0].length > 0 && parts[1].includes(".");
}

function friendlyRegistrationError(error: unknown): string {
  const message = errorMessage(error);
  const normalized = message.toLowerCase();
  const translations: Array<[string, string]> = [
    ["email already registered", "这个邮箱已经注册，可以直接登录。"],
    ["invite code is invalid", "邀请码无效或已经失效。"],
    ["verification code is invalid or expired", "邮箱验证码错误或已经过期。"],
    ["verification code was sent recently", "验证码刚刚发送，请稍后再试。"],
    ["too many verification emails", "验证码发送次数过多，请稍后再试。"],
    ["email format is invalid", "请输入有效的邮箱地址。"],
    ["temporarily unavailable", "注册服务暂时不可用，请稍后再试。"],
  ];
  return translations.find(([needle]) => normalized.includes(needle))?.[1] ?? message;
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "请求失败，请稍后重试。";
}

