import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useAuthenticationStore } from "../application/authenticationStore";
import type { AuthenticationPort } from "../ports/AuthenticationPort";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { LoginView } from "./LoginView";

const port: AuthenticationPort = {
  restoreSession: async () => null,
  login: async () => { throw new Error("not used"); },
  sendRegistrationCode: async () => ({ expiresInSeconds: 600, resendAfterSeconds: 60 }),
  register: async () => { throw new Error("not used"); },
  logout: async () => undefined,
};

describe("LoginView accessibility contract", () => {
  beforeEach(() => {
    useAuthenticationStore.setState({
      phase: "signedOut",
      mode: "signIn",
      session: null,
      errorMessage: null,
      registrationMessage: null,
      isSendingRegistrationCode: false,
      registrationCodeCountdown: 0,
      shouldRetrySessionRestore: false,
    });
    useGeneralSettingsStore.setState({
      settings: {
        interfaceLanguage: "zh-CN",
        contextLanguage: "zh-CN",
        interfaceFontSize: 14,
        preventsIdleSystemSleep: false,
        revision: 1,
      },
    });
  });

  afterEach(cleanup);

  it("associates sign-in labels and preserves keyboard order", () => {
    render(<LoginView authentication={port} />);

    expect(screen.getByLabelText("账号")).toHaveAttribute("id", "auth-username");
    expect(screen.getByLabelText("密码")).toHaveAttribute("id", "auth-password");
    expect(screen.getByLabelText("账号")).toHaveFocus();
    expect(Array.from(document.querySelectorAll("form input, form button")).map((element) => element.id || element.textContent?.trim())).toEqual([
      "auth-username",
      "auth-password",
      "登录",
      "创建账号",
    ]);
  });

  it("associates every registration field without changing its tab order", () => {
    useAuthenticationStore.getState().showRegistration();
    render(<LoginView authentication={port} />);

    expect(screen.getByLabelText("邮箱")).toHaveAttribute("id", "auth-username");
    expect(screen.getByLabelText("邀请码")).toHaveAttribute("id", "auth-invite-code");
    expect(screen.getByLabelText("邮箱验证码")).toHaveAttribute("id", "auth-verification-code");
    expect(screen.getByLabelText("密码")).toHaveAttribute("id", "auth-password");
    expect(screen.getByLabelText("确认密码")).toHaveAttribute("id", "auth-confirm-password");
    expect(screen.getByLabelText("邮箱")).toHaveFocus();
    expect(Array.from(document.querySelectorAll("form input, form button")).map((element) => element.id || element.textContent?.trim())).toEqual([
      "auth-username",
      "auth-invite-code",
      "auth-verification-code",
      "发送验证码",
      "auth-password",
      "auth-confirm-password",
      "创建账号",
      "返回登录",
    ]);
  });
});
