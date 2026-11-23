import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceIndexSnapshot } from "../../workspace-index/domain/workspaceIndex";
import { PetQuickChatSurface } from "./PetQuickChatSurface";

const resources: WorkspaceIndexSnapshot = {
  contacts: [{ id: "contact-1", title: "叽咕狸", subtitle: "最近会话", conversationId: "conversation-1" }],
  projects: [{ id: "project-1", title: "ChatOS", subtitle: "项目消息", conversationId: null }],
};

describe("PetQuickChatSurface", () => {
  afterEach(cleanup);

  it("offers quick actions and owner-scoped workspace resources", () => {
    const onClose = vi.fn();
    const onOpenTranslation = vi.fn();
    const onOpenNotepad = vi.fn();
    const onSelectResource = vi.fn();
    render(
      <PetQuickChatSurface
        onClose={onClose}
        onOpenNotepad={onOpenNotepad}
        onOpenTranslation={onOpenTranslation}
        onSelectResource={onSelectResource}
        resources={resources}
      />,
    );

    expect(screen.getByRole("dialog", { name: "快捷聊天" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /快速翻译/ }));
    expect(onClose).toHaveBeenCalledOnce();
    expect(onOpenTranslation).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: /ChatOS/ }));
    expect(onSelectResource).toHaveBeenCalledWith("project", "project-1");
  });

  it("keeps quick actions available when there are no resources", () => {
    render(
      <PetQuickChatSurface
        onClose={vi.fn()}
        onOpenNotepad={vi.fn()}
        onOpenTranslation={vi.fn()}
        onSelectResource={vi.fn()}
        resources={{ contacts: [], projects: [] }}
      />,
    );
    expect(screen.getByText("还没有可用的联系人或项目。你仍可以使用上方的快速功能。"))
      .toBeVisible();
    expect(screen.getByRole("button", { name: /快速记事本/ })).toBeVisible();
  });
});
