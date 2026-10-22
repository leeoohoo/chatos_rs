import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GeneralSettingsPort } from "../ports/GeneralSettingsPort";
import { useGeneralSettingsStore } from "../application/generalSettingsStore";
import { GeneralSettingsView } from "./GeneralSettingsView";

describe("GeneralSettingsView", () => {
  beforeEach(() => useGeneralSettingsStore.getState().reset());

  it("renders the macOS-derived cards and switches its language immediately", async () => {
    const port: GeneralSettingsPort = {
      load: vi.fn(async () => ({ interfaceLanguage: "zh-CN" as const, contextLanguage: "zh-CN" as const, interfaceFontSize: 14, preventsIdleSystemSleep: false, revision: 1 })),
      update: vi.fn(async (_owner, value) => ({ ...value, revision: value.revision + 1 })),
    };
    render(<GeneralSettingsView displayName="Alice" onLogout={vi.fn()} ownerUserId="owner-1" port={port} role="user" username="alice" />);
    expect(await screen.findByRole("heading", { name: "常规与账号" })).toBeInTheDocument();
    expect(screen.getByText("运行与电源")).toBeInTheDocument();
    fireEvent.change(screen.getAllByLabelText("Language")[0]!, { target: { value: "en-US" } });
    expect(screen.getByRole("heading", { name: "General & Account" })).toBeInTheDocument();
    await waitFor(() => expect(port.update).toHaveBeenCalled());
  });
});
