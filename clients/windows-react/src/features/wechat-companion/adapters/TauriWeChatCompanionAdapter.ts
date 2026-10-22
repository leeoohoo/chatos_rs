import { invoke } from "@tauri-apps/api/core";
import { weChatBindTicketSchema, weChatCompanionSnapshotSchema } from "../domain/wechatCompanion";
import type { WeChatCompanionPort } from "../ports/WeChatCompanionPort";

export class TauriWeChatCompanionAdapter implements WeChatCompanionPort {
  async load(ownerUserId: string) {
    return weChatCompanionSnapshotSchema.parse(await invoke("load_wechat_companion", { ownerUserId }));
  }

  async issueTicket(ownerUserId: string) {
    return weChatBindTicketSchema.parse(await invoke("issue_wechat_bind_ticket", { ownerUserId }));
  }

  async pollTicket(ownerUserId: string) {
    return weChatBindTicketSchema.parse(await invoke("poll_wechat_bind_ticket", { ownerUserId }));
  }

  async confirmTicket(ownerUserId: string) {
    await invoke("confirm_wechat_bind_ticket", { ownerUserId });
  }

  async revokeSession(ownerUserId: string, sessionId: string) {
    await invoke("revoke_wechat_client_session", { input: { ownerUserId, sessionId } });
  }

  async unbind(ownerUserId: string) {
    await invoke("unbind_wechat_companion", { ownerUserId });
  }

  async discardTicket(ownerUserId: string) {
    await invoke("discard_wechat_bind_ticket", { ownerUserId });
  }
}
