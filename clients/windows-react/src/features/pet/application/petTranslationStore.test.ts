import { beforeEach, describe, expect, it, vi } from "vitest";
import { usePetTranslationStore } from "./petTranslationStore";
import type { PetTranslationPort } from "../ports/PetTranslationPort";
import type { AuthSession } from "../../authentication/domain/authentication";

const session: AuthSession = { user: { id: "owner-1", username: "owner", displayName: null, role: "user" } };

describe("petTranslationStore", () => {
  beforeEach(() => {
    usePetTranslationStore.setState({
      session: null,
      records: [],
      sourceText: "",
      translatedText: "",
      target: "simplifiedChinese",
      outputStyle: "translationOnly",
      isLoading: false,
      isTranslating: false,
      error: null,
      selectedRecordId: null,
    });
  });

  it("keeps the translated record selected and prepends it to history", async () => {
    const port: PetTranslationPort = {
      list: vi.fn(async () => ({ records: [] })),
      translate: vi.fn(async () => ({
        id: "550e8400-e29b-41d4-a716-446655440000",
        sourceText: "hello",
        translatedText: "你好",
        target: "simplifiedChinese",
        outputStyle: "translationOnly",
        createdAtUnixMs: 1,
      } as const)),
      clear: vi.fn(async () => ({ records: [] })),
      close: vi.fn(async () => undefined),
    };
    await usePetTranslationStore.getState().load(port, session);
    usePetTranslationStore.getState().setSourceText("hello");
    await usePetTranslationStore.getState().translate(port);
    const state = usePetTranslationStore.getState();
    expect(state.translatedText).toBe("你好");
    expect(state.records).toHaveLength(1);
    expect(state.selectedRecordId).toBe(state.records[0]?.id);
  });

  it("does not send an empty source to the connector", async () => {
    const translate = vi.fn();
    const port = {
      list: vi.fn(async () => ({ records: [] })),
      translate,
      clear: vi.fn(async () => ({ records: [] })),
      close: vi.fn(async () => undefined),
    } as unknown as PetTranslationPort;
    await usePetTranslationStore.getState().load(port, session);
    await usePetTranslationStore.getState().translate(port);
    expect(translate).not.toHaveBeenCalled();
    expect(usePetTranslationStore.getState().error).toBe("请输入要翻译的内容。");
  });
});
