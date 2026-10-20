import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAskUserStore } from "../application/askUserStore";
import { sampleAskUserPrompt } from "../../../test/askUserFixture";
import type { AskUserPort } from "../ports/AskUserPort";
import { AskUserPromptCard } from "./AskUserPromptCard";

describe("AskUserPromptCard", () => {
  afterEach(cleanup);

  beforeEach(() => {
    useAskUserStore.getState().reset("user-1");
    useAskUserStore.setState({ conversationId: "conversation-1", hasLoaded: true });
  });

  it("enforces required fields and maximum choice count before submit", async () => {
    const prompt = sampleAskUserPrompt();
    const port: AskUserPort = {
      loadPrompts: vi.fn(async () => [prompt]),
      submitPrompt: vi.fn(async () => ({ ...prompt, status: "ok" as const })),
      cancelPrompt: vi.fn(async () => ({ ...prompt, status: "canceled" as const })),
    };
    render(<AskUserPromptCard prompt={prompt} port={port} onResolved={vi.fn(async () => {})} />);
    const submit = screen.getByRole("button", { name: "确认提交" });
    expect(submit).toBeDisabled();
    fireEvent.change(screen.getByLabelText(/Name/), { target: { value: "Ada" } });
    fireEvent.click(screen.getByRole("button", { name: /One/ }));
    expect(submit).toBeEnabled();
    expect(screen.getByRole("button", { name: /Two/ })).toBeDisabled();
    fireEvent.click(submit);
    await waitFor(() => expect(port.submitPrompt).toHaveBeenCalledOnce());
    expect(useAskUserStore.getState().prompts[0]?.status).toBe("ok");
  });

  it("labels secrets as unsupported by IPC", () => {
    render(<AskUserPromptCard
      prompt={sampleAskUserPrompt()}
      port={{ loadPrompts: vi.fn(), submitPrompt: vi.fn(), cancelPrompt: vi.fn() }}
      onResolved={vi.fn(async () => {})}
    />);
    expect(screen.getByText("安全字段不能通过本机 IPC 提交")).toBeInTheDocument();
    expect(screen.getByLabelText(/Password/)).toHaveAttribute("type", "password");
  });
});
