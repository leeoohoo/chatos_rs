import { z } from "zod";

export const authUserSchema = z.object({
  id: z.string().min(1),
  username: z.string().min(1),
  displayName: z.string().nullable().optional(),
  role: z.string().min(1),
});

export const authSessionSchema = z.object({ user: authUserSchema });

export const registrationCodeDeliverySchema = z.object({
  expiresInSeconds: z.number().int(),
  resendAfterSeconds: z.number().int(),
});

export type AuthSession = z.infer<typeof authSessionSchema>;
export type RegistrationCodeDelivery = z.infer<typeof registrationCodeDeliverySchema>;
export type AuthenticationMode = "signIn" | "register";
export type AuthenticationPhase =
  | "restoring"
  | "signedOut"
  | "authenticating"
  | "authenticated";

export interface LoginSubmission {
  username: string;
  password: string;
}

export interface RegistrationCodeSubmission {
  email: string;
  inviteCode: string;
}

export interface RegistrationSubmission extends RegistrationCodeSubmission {
  password: string;
  verificationCode: string;
}

