import type { WeChatBindTicket, WeChatCompanionSnapshot } from "../domain/wechatCompanion";

export interface WeChatCompanionPort {
  load(ownerUserId: string): Promise<WeChatCompanionSnapshot>;
  issueTicket(ownerUserId: string): Promise<WeChatBindTicket>;
  pollTicket(ownerUserId: string): Promise<WeChatBindTicket>;
  confirmTicket(ownerUserId: string): Promise<void>;
  revokeSession(ownerUserId: string, sessionId: string): Promise<void>;
  unbind(ownerUserId: string): Promise<void>;
  discardTicket(ownerUserId: string): Promise<void>;
}
