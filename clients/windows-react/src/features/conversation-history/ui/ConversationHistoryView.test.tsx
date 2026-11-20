import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  ConversationMessage,
  formatConversationMessageTime,
} from "./ConversationHistoryView";

describe("ConversationMessage", () => {
  afterEach(cleanup);

  it("shows the macOS sender and semantic local timestamp for user messages", () => {
    const createdAt = new Date(2026, 9, 7, 10, 57).getTime();
    render(
      <ConversationMessage
        assistantName="叽咕狸"
        content="你好啊"
        createdAtUnixMs={createdAt}
        role="user"
      />,
    );

    expect(screen.getByText("你")).toBeInTheDocument();
    expect(screen.getByText("你好啊")).toBeInTheDocument();
    expect(screen.getByText(formatConversationMessageTime(createdAt))).toHaveAttribute(
      "datetime",
      new Date(createdAt).toISOString(),
    );
  });

  it("uses the owning contact or Agent name for assistant messages", () => {
    render(
      <ConversationMessage
        assistantName="代码审查 Agent"
        content={{ answer: "# 审查完成\n\n- 类型安全\n- 回归通过" }}
        createdAtUnixMs={1}
        role="assistant"
      />,
    );

    expect(screen.getByText("代码审查 Agent")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "审查完成" })).toBeInTheDocument();
    expect(screen.getByRole("list")).toHaveTextContent("类型安全回归通过");
    expect(screen.queryByText("叽咕狸")).not.toBeInTheDocument();
  });

  it("falls back safely when a timestamp is outside the Date range", () => {
    render(
      <ConversationMessage
        assistantName="叽咕狸"
        content="仍然显示正文"
        createdAtUnixMs={Number.MAX_SAFE_INTEGER}
        role="assistant"
      />,
    );

    expect(screen.getByText("时间未知")).toBeInTheDocument();
    expect(screen.getByText("时间未知")).not.toHaveAttribute("datetime");
    expect(screen.getByText("仍然显示正文")).toBeInTheDocument();
  });

  it("keeps message actions inside the message presentation boundary", () => {
    render(
      <ConversationMessage assistantName="叽咕狸" content="带操作" createdAtUnixMs={1} role="user">
        <button type="button">查看过程</button>
      </ConversationMessage>,
    );

    const article = screen.getByRole("article");
    expect(article).toContainElement(screen.getByRole("button", { name: "查看过程" }));
  });
});
