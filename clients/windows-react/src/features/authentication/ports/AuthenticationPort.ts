import type {
  AuthSession,
  LoginSubmission,
  RegistrationCodeDelivery,
  RegistrationCodeSubmission,
  RegistrationSubmission,
} from "../domain/authentication";

export interface AuthenticationPort {
  restoreSession(): Promise<AuthSession | null>;
  login(submission: LoginSubmission): Promise<AuthSession>;
  sendRegistrationCode(
    submission: RegistrationCodeSubmission,
  ): Promise<RegistrationCodeDelivery>;
  register(submission: RegistrationSubmission): Promise<AuthSession>;
  logout(): Promise<void>;
}

