import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AuthSession,
  LoginSubmission,
  RegistrationCodeDelivery,
  RegistrationCodeSubmission,
  RegistrationSubmission,
} from "../domain/authentication";
import type { AuthenticationPort } from "../ports/AuthenticationPort";
import { useAuthenticationStore } from "./authenticationStore";

const session: AuthSession = {
  user: {
    id: "user-1",
    username: "person@example.com",
    displayName: "Person",
    role: "user",
  },
};

beforeEach(() => {
  vi.useRealTimers();
  useAuthenticationStore.setState({
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
  });
});

describe("authentication application state", () => {
  it("restores an existing session", async () => {
    const port = new TestAuthenticationPort();
    port.restoredSession = session;

    await useAuthenticationStore.getState().start(port);

    expect(useAuthenticationStore.getState().phase).toBe("authenticated");
    expect(useAuthenticationStore.getState().session?.user.id).toBe("user-1");
    expect(port.restoreCalls).toBe(1);
  });

  it("keeps a late login response signed out and clears its credential", async () => {
    const port = new TestAuthenticationPort();
    const login = deferred<AuthSession>();
    port.loginResult = login.promise;

    const loginTask = useAuthenticationStore.getState().login(port, {
      username: "person@example.com",
      password: "secret-value",
    });
    await Promise.resolve();
    const logoutTask = useAuthenticationStore.getState().logout(port);

    expect(useAuthenticationStore.getState().phase).toBe("signedOut");
    expect(port.logoutCalls).toBe(0);
    login.resolve(session);
    await Promise.all([loginTask, logoutTask]);

    expect(useAuthenticationStore.getState().phase).toBe("signedOut");
    expect(useAuthenticationStore.getState().session).toBeNull();
    expect(port.logoutCalls).toBe(1);
  });

  it("waits for a pending logout before beginning a new login", async () => {
    const port = new TestAuthenticationPort();
    const logout = deferred<void>();
    port.logoutResult = logout.promise;

    const logoutTask = useAuthenticationStore.getState().logout(port);
    await Promise.resolve();
    const loginTask = useAuthenticationStore.getState().login(port, {
      username: "person@example.com",
      password: "secret-value",
    });
    await Promise.resolve();

    expect(port.loginCalls).toBe(0);
    logout.resolve();
    await Promise.all([logoutTask, loginTask]);

    expect(port.loginCalls).toBe(1);
    expect(useAuthenticationStore.getState().phase).toBe("authenticated");
  });

  it("uses the server-provided registration resend countdown", async () => {
    vi.useFakeTimers();
    const port = new TestAuthenticationPort();
    port.codeDelivery = { expiresInSeconds: 600, resendAfterSeconds: 3 };

    await useAuthenticationStore.getState().sendRegistrationCode(port, {
      email: "person@example.com",
      inviteCode: "invite-123",
    });

    expect(useAuthenticationStore.getState().registrationCodeCountdown).toBe(3);
    await vi.advanceTimersByTimeAsync(3_000);
    expect(useAuthenticationStore.getState().registrationCodeCountdown).toBe(0);
  });

  it("rejects an invalid registration before calling the adapter", async () => {
    const port = new TestAuthenticationPort();

    await useAuthenticationStore.getState().register(port, {
      email: "person@example.com",
      inviteCode: "invite-123",
      verificationCode: "12345x",
      password: "secret-value",
    });

    expect(port.registerCalls).toBe(0);
    expect(useAuthenticationStore.getState().errorMessage).toBe("请输入 6 位邮箱验证码。");
  });
});

class TestAuthenticationPort implements AuthenticationPort {
  restoredSession: AuthSession | null = null;
  loginResult: Promise<AuthSession> = Promise.resolve(session);
  logoutResult: Promise<void> = Promise.resolve();
  codeDelivery: RegistrationCodeDelivery = {
    expiresInSeconds: 600,
    resendAfterSeconds: 60,
  };
  restoreCalls = 0;
  loginCalls = 0;
  registerCalls = 0;
  logoutCalls = 0;

  async restoreSession(): Promise<AuthSession | null> {
    this.restoreCalls += 1;
    return this.restoredSession;
  }

  async login(_submission: LoginSubmission): Promise<AuthSession> {
    this.loginCalls += 1;
    return this.loginResult;
  }

  async sendRegistrationCode(
    _submission: RegistrationCodeSubmission,
  ): Promise<RegistrationCodeDelivery> {
    return this.codeDelivery;
  }

  async register(_submission: RegistrationSubmission): Promise<AuthSession> {
    this.registerCalls += 1;
    return session;
  }

  async logout(): Promise<void> {
    this.logoutCalls += 1;
    return this.logoutResult;
  }
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

