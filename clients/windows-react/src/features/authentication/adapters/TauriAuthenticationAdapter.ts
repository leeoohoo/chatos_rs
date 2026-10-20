import { invoke } from "@tauri-apps/api/core";
import {
  authSessionSchema,
  registrationCodeDeliverySchema,
  type AuthSession,
  type LoginSubmission,
  type RegistrationCodeDelivery,
  type RegistrationCodeSubmission,
  type RegistrationSubmission,
} from "../domain/authentication";
import type { AuthenticationPort } from "../ports/AuthenticationPort";

export class TauriAuthenticationAdapter implements AuthenticationPort {
  async restoreSession(): Promise<AuthSession | null> {
    const result = await invoke("restore_authentication_session");
    return result === null ? null : authSessionSchema.parse(result);
  }

  async login(submission: LoginSubmission): Promise<AuthSession> {
    return authSessionSchema.parse(
      await invoke("login", {
        username: submission.username,
        password: submission.password,
      }),
    );
  }

  async sendRegistrationCode(
    submission: RegistrationCodeSubmission,
  ): Promise<RegistrationCodeDelivery> {
    return registrationCodeDeliverySchema.parse(
      await invoke("send_registration_code", {
        email: submission.email,
        inviteCode: submission.inviteCode,
      }),
    );
  }

  async register(submission: RegistrationSubmission): Promise<AuthSession> {
    return authSessionSchema.parse(
      await invoke("register", {
        email: submission.email,
        password: submission.password,
        inviteCode: submission.inviteCode,
        verificationCode: submission.verificationCode,
      }),
    );
  }

  async logout(): Promise<void> {
    await invoke("logout");
  }
}

