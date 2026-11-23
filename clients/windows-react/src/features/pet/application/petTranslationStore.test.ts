import { beforeEach, describe, expect, it, vi } from "vitest";
import { usePetTranslationStore } from "./petTranslationStore";
import type { PetTranslationPort } from "../ports/PetTranslationPort";
import type { AuthSession } from "../../authentication/domain/authentication";
import type { PetTranslationRecord } from "../domain/petTranslation";

const session: AuthSession = { user: { id: "owner-1", username: "owner", displayName: null, role: "user" } };

describe("petTranslationStore", () => {
  beforeEach(() => {
    usePetTranslationStore.setState({
      session: null,
      loadRequestId: 0,
      records: [],
      sourceText: "",
      translatedText: "",
      target: "simplifiedChinese",
      outputStyle: "translationOnly",
      isLoading: false,
      isTranslating: false,
      error: null,
      selectedRecordId: null,
      attachments: [],
      attachmentError: null,
      isImportingAttachments: false,
    });
  });

  it("keeps the translated record selected and prepends it to history", async () => {
    const port: PetTranslationPort = {
      list: vi.fn(async () => ({ records: [] })),
      translate: vi.fn(async (): Promise<PetTranslationRecord> => ({
        id: "550e8400-e29b-41d4-a716-446655440000",
        sourceText: "hello",
        translatedText: "你好",
        attachments: [],
        target: "simplifiedChinese",
        outputStyle: "translationOnly",
        createdAtUnixMs: 1,
      })),
      clear: vi.fn(async () => ({ records: [] })),
      pickAttachments: vi.fn(async () => ({ scope: { ownerUserId: "owner-1", conversationId: "pet-translation", draftGeneration: 0 }, attachments: [], errors: [] })),
      importClipboard: vi.fn(async () => ({ kind: "empty" as const, text: null, result: null })),
      discardAttachments: vi.fn(async () => undefined),
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
    expect(usePetTranslationStore.getState().error).toBe("请输入要翻译的内容或添加图片。");
  });

  it("keeps image attachments and discards unsupported imports", async () => {
    const discardAttachments = vi.fn(async () => undefined);
    const port = {
      list: vi.fn(async () => ({ records: [] })),
      translate: vi.fn(),
      clear: vi.fn(async () => ({ records: [] })),
      pickAttachments: vi.fn(async () => ({
        scope: { ownerUserId: "owner-1", conversationId: "pet-translation", draftGeneration: 0 },
        attachments: [
          { id: "image-1", name: "screenshot.png", mimeType: "image/png", kind: "image", origin: "pasted_image", byteSize: 4, previewUrl: null },
          { id: "file-1", name: "notes.txt", mimeType: "text/plain", kind: "file", origin: "file", byteSize: 4, previewUrl: null },
        ],
        errors: [],
      })),
      importClipboard: vi.fn(),
      discardAttachments,
      close: vi.fn(async () => undefined),
    } as unknown as PetTranslationPort;
    await usePetTranslationStore.getState().load(port, session);
    await usePetTranslationStore.getState().pickAttachments(port);
    expect(usePetTranslationStore.getState().attachments.map(({ id }) => id)).toEqual(["image-1"]);
    expect(discardAttachments).toHaveBeenCalledWith(
      { ownerUserId: "owner-1", conversationId: "pet-translation", draftGeneration: 0 },
      ["file-1"],
    );
    expect(usePetTranslationStore.getState().attachmentError).toContain("仅支持");
  });

  it("submits image-only drafts with their attachment ids", async () => {
    const translate = vi.fn(async () => ({
      id: "550e8400-e29b-41d4-a716-446655440001",
      sourceText: "",
      translatedText: "识别结果",
      attachments: [{ name: "screenshot.png", mimeType: "image/png", isImage: true as const }],
      target: "simplifiedChinese" as const,
      outputStyle: "translationOnly" as const,
      createdAtUnixMs: 2,
    }));
    const port = {
      list: vi.fn(async () => ({ records: [] })),
      translate,
      clear: vi.fn(async () => ({ records: [] })),
      pickAttachments: vi.fn(),
      importClipboard: vi.fn(),
      discardAttachments: vi.fn(async () => undefined),
      close: vi.fn(async () => undefined),
    } as unknown as PetTranslationPort;
    await usePetTranslationStore.getState().load(port, session);
    usePetTranslationStore.setState({
      attachments: [{ id: "image-1", name: "screenshot.png", mimeType: "image/png", kind: "image", origin: "pasted_image", byteSize: 4, previewUrl: null }],
    });
    await usePetTranslationStore.getState().translate(port);
    expect(translate).toHaveBeenCalledWith(expect.objectContaining({ sourceText: "", attachmentIds: ["image-1"] }));
    expect(usePetTranslationStore.getState().attachments).toEqual([]);
  });

  it("clears and discards a previous account's draft when the session changes", async () => {
    const discardAttachments = vi.fn(async () => undefined);
    const port = {
      list: vi.fn(async () => ({ records: [] })),
      translate: vi.fn(),
      clear: vi.fn(async () => ({ records: [] })),
      pickAttachments: vi.fn(),
      importClipboard: vi.fn(),
      discardAttachments,
      close: vi.fn(async () => undefined),
    } as unknown as PetTranslationPort;
    await usePetTranslationStore.getState().load(port, session);
    usePetTranslationStore.setState({
      sourceText: "private draft",
      translatedText: "私有译文",
      attachments: [{ id: "old-image", name: "old.png", mimeType: "image/png", kind: "image", origin: "pasted_image", byteSize: 4, previewUrl: null }],
    });

    const nextSession: AuthSession = { user: { ...session.user, id: "owner-2" } };
    await usePetTranslationStore.getState().load(port, nextSession);

    expect(discardAttachments).toHaveBeenCalledWith(
      { ownerUserId: "owner-1", conversationId: "pet-translation", draftGeneration: 0 },
      ["old-image"],
    );
    expect(usePetTranslationStore.getState().session?.user.id).toBe("owner-2");
    expect(usePetTranslationStore.getState().sourceText).toBe("");
    expect(usePetTranslationStore.getState().translatedText).toBe("");
    expect(usePetTranslationStore.getState().attachments).toEqual([]);
  });

  it("does not let a stale history load overwrite a newer account", async () => {
    let resolveFirst: ((value: { records: [] }) => void) | undefined;
    const firstList = new Promise<{ records: [] }>((resolve) => { resolveFirst = resolve; });
    const port = {
      list: vi.fn()
        .mockReturnValueOnce(firstList)
        .mockResolvedValueOnce({ records: [] }),
      translate: vi.fn(),
      clear: vi.fn(async () => ({ records: [] })),
      pickAttachments: vi.fn(),
      importClipboard: vi.fn(),
      discardAttachments: vi.fn(async () => undefined),
      close: vi.fn(async () => undefined),
    } as unknown as PetTranslationPort;
    const firstLoad = usePetTranslationStore.getState().load(port, session);
    const nextSession: AuthSession = { user: { ...session.user, id: "owner-2" } };
    const secondLoad = usePetTranslationStore.getState().load(port, nextSession);
    resolveFirst?.({ records: [] });
    await Promise.all([firstLoad, secondLoad]);

    expect(usePetTranslationStore.getState().session?.user.id).toBe("owner-2");
    expect(usePetTranslationStore.getState().loadRequestId).toBe(2);
    expect(usePetTranslationStore.getState().isLoading).toBe(false);
  });
});
