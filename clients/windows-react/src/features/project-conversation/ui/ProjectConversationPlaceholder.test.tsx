import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProjectConversationPlaceholder } from "./ProjectConversationPlaceholder";

describe("ProjectConversationPlaceholder", () => {
  it("surfaces the Host failure and exposes recovery while the project is selected", () => {
    const retry = vi.fn();
    const port = { prepareProjectConversation: vi.fn() };
    render(
      <ProjectConversationPlaceholder
        ownerUserId="owner-1"
        project={{ id: "project-1", title: "chatos", subtitle: null, conversationId: null }}
        hostReady={false}
        hostDetail="账号没有已启用且带凭据的模型。"
        onHostRetry={retry}
        defaultContactReady={false}
        port={port}
        historyPort={{} as never}
        composerPort={{} as never}
        attachmentPort={{} as never}
        realtimePort={{} as never}
        runtimeSettingsPort={{} as never}
        runInspectorPort={{} as never}
        askUserPort={{} as never}
      />,
    );

    expect(screen.getByRole("heading", { name: "本地服务不可用" })).toBeVisible();
    expect(screen.getByText("账号没有已启用且带凭据的模型。")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "重新检查" }));
    expect(retry).toHaveBeenCalledOnce();
    expect(port.prepareProjectConversation).not.toHaveBeenCalled();
  });
});
