import { zodResolver } from "@hookform/resolvers/zod";
import { useEffect, useMemo, type ReactNode } from "react";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { useAuthenticationStore } from "../application/authenticationStore";
import type { AuthenticationPort } from "../ports/AuthenticationPort";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import type { ChatOSLanguage } from "../../general-settings/domain/generalSettings";
import { interfaceText } from "../../../shared/interfaceLocale";

const authenticationFormSchema = (language: ChatOSLanguage) => z
  .object({
    username: z.string(),
    inviteCode: z.string(),
    verificationCode: z.string(),
    password: z.string(),
    confirmPassword: z.string(),
  })
  .superRefine((form, context) => {
    if (!form.username.trim()) {
      context.addIssue({ code: "custom", path: ["username"], message: interfaceText(language, "accountRequired") });
    }
  });

type AuthenticationForm = z.infer<ReturnType<typeof authenticationFormSchema>>;

interface LoginViewProps {
  authentication: AuthenticationPort;
}

export function LoginView({ authentication }: LoginViewProps) {
  const language = useGeneralSettingsStore((state) => state.settings.interfaceLanguage);
  const text = (key: Parameters<typeof interfaceText>[1]) => interfaceText(language, key);
  const formSchema = useMemo(() => authenticationFormSchema(language), [language]);
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
        setError("password", { message: text("passwordRequired") });
        return;
      }
      await login(authentication, { username: form.username, password: form.password });
      return;
    }
    if (form.password !== form.confirmPassword) {
      setError("confirmPassword", { message: text("passwordMismatch") });
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
          <h1>{text("authHeadlineFirst")}</h1>
          <h1>{text("authHeadlineSecond")}</h1>
        </div>
        <p>{text("authTagline")}</p>
      </div>

      <div className="authentication-form-surface">
        <form className="authentication-form" onSubmit={submit} noValidate>
          <header>
            <h2>{mode === "signIn" ? text("signIn") : text("createAccount")}</h2>
            <p>{mode === "signIn" ? text("signInSubtitle") : text("registerSubtitle")}</p>
          </header>

          <AuthField
            label={mode === "signIn" ? text("account") : text("email")}
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
              <AuthField label={text("inviteCode")} error={errors.inviteCode?.message}>
                <input
                  autoComplete="off"
                  placeholder={text("inviteCodePlaceholder")}
                  {...register("inviteCode")}
                />
              </AuthField>
              <AuthField label={text("emailCode")} error={errors.verificationCode?.message}>
                <div className="verification-row">
                  <input
                    autoComplete="one-time-code"
                    inputMode="numeric"
                    maxLength={6}
                    placeholder={text("emailCodePlaceholder")}
                    {...register("verificationCode")}
                  />
                  <button
                    className="secondary-button verification-button"
                    disabled={isSendingCode || countdown > 0 || isAuthenticating}
                    onClick={() => void requestCode()}
                    type="button"
                  >
                    {isSendingCode ? text("sending") : countdown > 0 ? `${countdown}s` : text("sendCode")}
                  </button>
                </div>
              </AuthField>
            </>
          )}

          <AuthField label={text("password")} error={errors.password?.message}>
            <input
              autoComplete={mode === "signIn" ? "current-password" : "new-password"}
              placeholder={mode === "signIn" ? text("password") : text("passwordLength")}
              type="password"
              {...register("password")}
            />
          </AuthField>

          {mode === "register" && (
            <AuthField label={text("confirmPassword")} error={errors.confirmPassword?.message}>
              <input
                autoComplete="new-password"
                placeholder={text("confirmPasswordPlaceholder")}
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
            {isAuthenticating ? <span className="button-spinner" aria-label={text("processing")} /> : mode === "signIn" ? text("signIn") : text("createAccount")}
          </button>

          <p className="mode-switch">
            {mode === "signIn" ? text("noAccount") : text("hasAccount")}
            <button
              onClick={mode === "signIn" ? showRegistration : showLogin}
              type="button"
            >
              {mode === "signIn" ? text("createAccount") : text("backToSignIn")}
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

