import { useEffect, type ReactNode } from "react";
import { useAuthenticationStore } from "../application/authenticationStore";
import type { AuthSession } from "../domain/authentication";
import type { AuthenticationPort } from "../ports/AuthenticationPort";
import { LoginView } from "./LoginView";

interface AuthenticationGateProps {
  authentication: AuthenticationPort;
  children(session: AuthSession): ReactNode;
}

export function AuthenticationGate({
  authentication,
  children,
}: AuthenticationGateProps) {
  const phase = useAuthenticationStore((state) => state.phase);
  const session = useAuthenticationStore((state) => state.session);
  const start = useAuthenticationStore((state) => state.start);
  const retrySessionRestoreIfNeeded = useAuthenticationStore(
    (state) => state.retrySessionRestoreIfNeeded,
  );

  useEffect(() => {
    void start(authentication);
  }, [authentication, start]);

  useEffect(() => {
    const retry = () => void retrySessionRestoreIfNeeded(authentication);
    window.addEventListener("focus", retry);
    return () => window.removeEventListener("focus", retry);
  }, [authentication, retrySessionRestoreIfNeeded]);

  if (phase === "restoring") {
    return (
      <section className="authentication-restoring" aria-live="polite">
        <span className="auth-sparkle" aria-hidden="true">✦</span>
        <span className="spinner" aria-hidden="true" />
        <p>正在恢复登录状态…</p>
      </section>
    );
  }

  if (phase === "authenticated" && session) return children(session);
  return <LoginView authentication={authentication} />;
}

