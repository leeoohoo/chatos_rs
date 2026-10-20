import { zodResolver } from "@hookform/resolvers/zod";
import { useEffect, type ReactNode } from "react";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { useAuthenticationStore } from "../application/authenticationStore";
import type { AuthenticationPort } from "../ports/AuthenticationPort";

const formSchema = z
  .object({
    username: z.string(),
    inviteCode: z.string(),
    verificationCode: z.string(),
    password: z.string(),
    confirmPassword: z.string(),
  })
  .superRefine((form, context) => {
    if (!form.username.trim()) {
      context.addIssue({ code: "custom", path: ["username"], message: "请输入账号。" });
    }
  });

type AuthenticationForm = z.infer<typeof formSchema>;

interface LoginViewProps {
  authentication: AuthenticationPort;
}

export function LoginView({ authentication }: LoginViewProps) {
  const phase = useAuthenticationStore((state) => state.phase);
  const mode = useAuthenticationStore((state) => state.mode);
  const errorMessage = useAuthenticationStore((state) => state.errorMessage);
  const registrationMessage = useAuthenticationStore((state) => state.registrationMessage);
  const isSendingCode = useAuthenticationStore((state) => state.isSendingRegistrationCode);
  const countdown = useAuthenticationStore((state) => state.registrationCodeCountdown);
  const login = useAuthenticationStore((state) => state.login);
  const registerAccount = useAuthenticationStore((state) => state.register);
  const sendRegistrationCode = useAuthenticationStore((state) => state.sendRegistrationCode);
  const showRegistration = useAuthenticationStore((state) => state.showRegistration);
  const showLogin = useAuthenticationStore((state) => state.showLogin);
  const isAuthenticating = phase === "authenticating";
  const {
    register,
    handleSubmit,
    getValues,
    resetField,
    setError,
    formState: { errors },
  } = useForm<AuthenticationForm>({
    resolver: zodResolver(formSchema),
    defaultValues: {
      username: "",
      inviteCode: "",
      verificationCode: "",
      password: "",
      confirmPassword: "",
    },
  });

  useEffect(() => {
    resetField("password");
    resetField("confirmPassword");
  }, [mode, resetField]);

  const submit = handleSubmit(async (form) => {
    if (mode === "signIn") {
      if (!form.password) {
        setError("password", { message: "请输入密码。" });
        return;
      }
      await login(authentication, { username: form.username, password: form.password });
      return;
    }
    if (form.password !== form.confirmPassword) {
      setError("confirmPassword", { message: "两次输入的密码不一致。" });
      return;
    }
    await registerAccount(authentication, {
      email: form.username,
      inviteCode: form.inviteCode,
      verificationCode: form.verificationCode,
      password: form.password,
    });
  });

  const requestCode = async () => {
    const form = getValues();
    await sendRegistrationCode(authentication, {
      email: form.username,
      inviteCode: form.inviteCode,
    });
  };

  return (
    <section className="authentication-screen">
      <div className="authentication-introduction">
        <div className="auth-orb" aria-hidden="true">
          <span>✦</span>
        </div>
        <div className="auth-headline">
          <h1>路还很长，</h1>
          <h1>我记得我们走到了哪里。</h1>
        </div>
        <p>以岁月打磨，致敬每一份认真。</p>
      </div>

      <div className="authentication-form-surface">
        <form className="authentication-form" onSubmit={submit} noValidate>
          <header>
            <h2>{mode === "signIn" ? "登录" : "创建账号"}</h2>
            <p>{mode === "signIn" ? "使用 ChatOS 平台账号继续" : "邀请测试 · 邮箱验证"}</p>
          </header>

          <AuthField
            label={mode === "signIn" ? "账号" : "邮箱"}
            error={errors.username?.message}
          >
            <input
              autoFocus
              autoComplete="username"
              placeholder="name@example.com"
              {...register("username")}
            />
          </AuthField>

          {mode === "register" && (
            <>
              <AuthField label="邀请码" error={errors.inviteCode?.message}>
                <input
                  autoComplete="off"
                  placeholder="输入邀请测试码"
                  {...register("inviteCode")}
                />
              </AuthField>
              <AuthField label="邮箱验证码" error={errors.verificationCode?.message}>
                <div className="verification-row">
                  <input
                    autoComplete="one-time-code"
                    inputMode="numeric"
                    maxLength={6}
                    placeholder="6 位验证码"
                    {...register("verificationCode")}
                  />
                  <button
                    className="secondary-button verification-button"
                    disabled={isSendingCode || countdown > 0 || isAuthenticating}
                    onClick={() => void requestCode()}
                    type="button"
                  >
                    {isSendingCode ? "发送中" : countdown > 0 ? `${countdown}s` : "发送验证码"}
                  </button>
                </div>
              </AuthField>
            </>
          )}

          <AuthField label="密码" error={errors.password?.message}>
            <input
              autoComplete={mode === "signIn" ? "current-password" : "new-password"}
              placeholder={mode === "signIn" ? "密码" : "12–128 个字符"}
              type="password"
              {...register("password")}
            />
          </AuthField>

          {mode === "register" && (
            <AuthField label="确认密码" error={errors.confirmPassword?.message}>
              <input
                autoComplete="new-password"
                placeholder="再次输入密码"
                type="password"
                {...register("confirmPassword")}
              />
            </AuthField>
          )}

          <div className="authentication-messages" aria-live="polite">
            {errorMessage && <p className="form-message form-message--error">● {errorMessage}</p>}
            {registrationMessage && <p className="form-message form-message--success">● {registrationMessage}</p>}
          </div>

          <button className="primary-button auth-submit" disabled={isAuthenticating} type="submit">
            {isAuthenticating ? <span className="button-spinner" aria-label="处理中" /> : mode === "signIn" ? "登录" : "创建账号"}
          </button>

          <p className="mode-switch">
            {mode === "signIn" ? "还没有账号？" : "已有账号？"}
            <button
              onClick={mode === "signIn" ? showRegistration : showLogin}
              type="button"
            >
              {mode === "signIn" ? "创建账号" : "返回登录"}
            </button>
          </p>
        </form>
      </div>
    </section>
  );
}

function AuthField({
  label,
  error,
  children,
}: {
  label: string;
  error?: string;
  children: ReactNode;
}) {
  return (
    <label className="auth-field">
      <span>{label}</span>
      {children}
      {error && <small>{error}</small>}
    </label>
  );
}

