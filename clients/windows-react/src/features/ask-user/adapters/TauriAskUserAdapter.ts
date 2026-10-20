import { invoke } from "@tauri-apps/api/core";
import { askUserPromptSchema, askUserPromptsSchema } from "../domain/askUser";
import type { AskUserMutationRequest, AskUserPort } from "../ports/AskUserPort";

export class TauriAskUserAdapter implements AskUserPort {
  async loadPrompts(ownerUserId: string, conversationId: string) {
    return askUserPromptsSchema.parse(
      await invoke("load_ask_user_prompts", { ownerUserId, conversationId }),
    );
  }

  async submitPrompt(request: AskUserMutationRequest & { submission: NonNullable<AskUserMutationRequest["submission"]> }) {
    return askUserPromptSchema.parse(
      await invoke("submit_ask_user_prompt", { input: request }),
    );
  }

  async cancelPrompt(request: AskUserMutationRequest) {
    return askUserPromptSchema.parse(
      await invoke("cancel_ask_user_prompt", { input: request }),
    );
  }
}
