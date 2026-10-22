import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { pluginSettingsPort } from "../../../test/pluginSettingsFixture";
import { usePluginSettingsStore } from "../application/pluginSettingsStore";
import { PluginSettingsView } from "./PluginSettingsView";

describe("PluginSettingsView", () => {
  afterEach(cleanup);

  beforeEach(() => {
    usePluginSettingsStore.getState().reset();
    useGeneralSettingsStore.setState((state) => ({ settings: { ...state.settings, interfaceLanguage: "zh-CN" } }));
  });

  it("shows installed Skills and keeps unadmitted install actions closed", async () => {
    render(<PluginSettingsView ownerUserId="owner-1" port={pluginSettingsPort()} onOpenInstallationAssistant={vi.fn()} />);
    expect(await screen.findByText("Diagram Studio")).toBeInTheDocument();
    expect(screen.getByText("更新未准入")).toBeDisabled();
    fireEvent.click(screen.getByText("Skills 与权限"));
    expect(screen.getByText("diagram-architecture")).toBeInTheDocument();
    expect(screen.getByText("process.spawn")).toBeInTheDocument();
  });

  it("filters by Skill and opens the installation assistant", async () => {
    const onOpen = vi.fn();
    render(<PluginSettingsView ownerUserId="owner-1" port={pluginSettingsPort()} onOpenInstallationAssistant={onOpen} />);
    await screen.findByText("Diagram Studio");
    fireEvent.change(screen.getByLabelText("搜索 Plugin"), { target: { value: "flowchart" } });
    expect(screen.getByText("Diagram Studio")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /安装助手/ }));
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it("updates the native enabled state", async () => {
    const port = pluginSettingsPort();
    render(<PluginSettingsView ownerUserId="owner-1" port={port} onOpenInstallationAssistant={vi.fn()} />);
    const toggle = await screen.findByLabelText("Diagram Studio enabled");
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).not.toBeChecked());
  });
});
