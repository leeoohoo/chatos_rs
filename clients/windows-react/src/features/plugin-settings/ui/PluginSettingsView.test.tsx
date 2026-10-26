import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { pluginSettingsPort } from "../../../test/pluginSettingsFixture";
import { pluginSettingsSnapshot } from "../../../test/pluginSettingsFixture";
import { usePluginSettingsStore } from "../application/pluginSettingsStore";
import { PluginSettingsView } from "./PluginSettingsView";

describe("PluginSettingsView", () => {
  afterEach(cleanup);

  beforeEach(() => {
    usePluginSettingsStore.getState().reset();
    useGeneralSettingsStore.setState((state) => ({ settings: { ...state.settings, interfaceLanguage: "zh-CN" } }));
  });

  it("shows installed Skills and opens the verified Marketplace update action", async () => {
    render(<PluginSettingsView ownerUserId="owner-1" port={pluginSettingsPort()} onOpenInstallationAssistant={vi.fn()} />);
    expect(await screen.findByText("Diagram Studio")).toBeInTheDocument();
    expect(screen.getByText("安全更新")).toBeEnabled();
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

  it("shows validated Hooks without claiming the Windows runner is active", async () => {
    const snapshot = pluginSettingsSnapshot();
    snapshot.plugins[0]!.hookCount = 1;
    snapshot.plugins[0]!.hookRuntimeStatus = "runner_unavailable";
    const port = pluginSettingsPort({ load: vi.fn(async () => snapshot) });
    render(<PluginSettingsView ownerUserId="owner-1" port={port} onOpenInstallationAssistant={vi.fn()} />);

    expect(await screen.findByText("Hook runner 未接入")).toBeInTheDocument();
    expect(screen.getByText("1 Hook Bundle")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Skills 与权限"));
    expect(screen.getByText("生命周期 Hooks")).toBeInTheDocument();
    expect(screen.getByText(/这些 Hook 当前不会运行/)).toBeInTheDocument();
  });

  it("keeps plugin secrets local to the password dialog and clears them when closed", async () => {
    const snapshot = pluginSettingsSnapshot();
    snapshot.plugins[0]!.credentials = [{ name: "primary-token", configured: false }];
    const port = pluginSettingsPort({ load: vi.fn(async () => snapshot), updateCredentials: vi.fn(async () => ({ ...snapshot, plugins: snapshot.plugins.map((plugin) => ({ ...plugin, credentials: [{ name: "primary-token", configured: true }] })) })) });
    render(<PluginSettingsView ownerUserId="owner-1" port={port} onOpenInstallationAssistant={vi.fn()} />);
    await screen.findByText("需要配置凭据");
    fireEvent.click(screen.getByRole("button", { name: "配置凭据…" }));
    const input = screen.getByLabelText("primary-token");
    expect(input).toHaveValue("");
    fireEvent.change(input, { target: { value: "secret-value" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭凭据配置" }));
    fireEvent.click(screen.getByRole("button", { name: "配置凭据…" }));
    expect(screen.getByLabelText("primary-token")).toHaveValue("");
    fireEvent.change(screen.getByLabelText("primary-token"), { target: { value: "new-secret" } });
    fireEvent.click(screen.getByRole("button", { name: "安全保存" }));
    await waitFor(() => expect(port.updateCredentials).toHaveBeenCalledWith(
      "owner-1",
      "plugin-1",
      { "primary-token": "new-secret" },
    ));
  });

  it("preflights a native folder before enabling manual installation", async () => {
    const port = pluginSettingsPort({
      pickManualDirectory: vi.fn(async () => ({ selectionId: "selection-private", directoryName: "safe-plugin" })),
      inspectManualDirectory: vi.fn(async (_ownerUserId, _presentationId, selectionId) => ({
        selectionId,
        directoryName: "safe-plugin",
        displayName: "Safe Plugin",
        version: "2.1.0",
        description: "Verified locally.",
        fingerprint: "b".repeat(64),
        components: [
          { kind: "skill" as const, displayName: "safe-skill", hookCount: null, events: [], runtimeStatus: null },
          { kind: "hook" as const, displayName: "lifecycle", hookCount: 3, events: ["SessionStart", "UserPromptSubmit"], runtimeStatus: "runner_unavailable" as const },
        ],
      })),
      installManualDirectory: vi.fn(async () => pluginSettingsPort().load("owner-1")),
    });
    render(<PluginSettingsView ownerUserId="owner-1" port={port} onOpenInstallationAssistant={vi.fn()} />);
    await screen.findByText("Diagram Studio");

    fireEvent.click(screen.getByRole("button", { name: "手动安装…" }));
    const installButton = screen.getByRole("button", { name: "确认安装" });
    expect(installButton).toBeDisabled();
    expect(screen.queryByText(/C:\\/)).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "选择根目录" }));
    expect(await screen.findByText("safe-plugin")).toBeInTheDocument();
    expect(installButton).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "预检" }));
    expect(await screen.findByText("Safe Plugin · 2.1.0")).toBeInTheDocument();
    expect(screen.getByText("safe-skill")).toBeInTheDocument();
    expect(screen.getByText("lifecycle")).toBeInTheDocument();
    expect(screen.getByText(/3 个处理器.*Windows runner 尚未接入/)).toBeInTheDocument();
    expect(screen.getByText("b".repeat(64))).toBeInTheDocument();
    expect(installButton).toBeEnabled();

    fireEvent.click(installButton);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(port.installManualDirectory).toHaveBeenCalledWith(
      "owner-1",
      expect.stringMatching(/^plugin-manual-/),
      "selection-private",
      "b".repeat(64),
    );
  });

  it("discards the native selection when the manual sheet closes", async () => {
    const port = pluginSettingsPort({ discardManualDirectory: vi.fn(async () => undefined) });
    render(<PluginSettingsView ownerUserId="owner-1" port={port} onOpenInstallationAssistant={vi.fn()} />);
    await screen.findByText("Diagram Studio");
    fireEvent.click(screen.getByRole("button", { name: "手动安装…" }));
    fireEvent.click(screen.getByRole("button", { name: "关闭手动安装" }));
    await waitFor(() => expect(port.discardManualDirectory).toHaveBeenCalledWith(
      "owner-1",
      expect.stringMatching(/^plugin-manual-/),
    ));
  });
});
