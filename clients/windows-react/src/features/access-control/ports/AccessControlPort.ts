import type { AccessControlSnapshot } from "../domain/accessControl";

export interface AccessControlPort {
  load(ownerUserId: string): Promise<AccessControlSnapshot>;
  updateApproval(ownerUserId: string, approvalPolicy: "on_request" | "never", approvalReviewer: "user" | "auto_review", riskAcknowledged: boolean): Promise<AccessControlSnapshot>;
}
