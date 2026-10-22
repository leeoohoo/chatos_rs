import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { aiModelSettingsPort } from "../../../test/aiModelSettingsFixture";
import { useAiModelSettingsStore } from "../application/aiModelSettingsStore";
import { AiModelSettingsView } from "./AiModelSettingsView";

describe("AiModelSettingsView", () => {
  beforeEach(() => useAiModelSettingsStore.getState().reset());

  it("renders macOS-derived defaults, task controls, and provider management", async () => {
    const port = aiModelSettingsPort();
    render(<AiModelSettingsView ownerUserId="owner-1" port={port} />);
    expect(await screen.findByText("统一模型配置")).toBeInTheDocument();
    expect(screen.getByText("Memory 总结")).toBeInTheDocument();
    expect(screen.getByText("任务执行模型")).toBeInTheDocument();
    expect(screen.getByLabelText("GPT 5 用于任务")).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "管理供应商" }));
    expect(screen.getByRole("dialog", { name: "模型供应商" })).toBeInTheDocument();
    expect(screen.getByText("凭据已保存")).toBeInTheDocument();
  });
});
