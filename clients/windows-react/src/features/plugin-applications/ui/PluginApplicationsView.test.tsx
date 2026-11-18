import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pluginApplicationFixture, pluginApplicationLaunchFixture } from "../../../test/pluginApplicationFixture";
import { usePluginApplicationStore } from "../application/pluginApplicationStore";
import type { PluginApplicationPort } from "../ports/PluginApplicationPort";
import { PluginApplicationHostView, PluginApplicationsView } from "./PluginApplicationsView";

beforeEach(() => usePluginApplicationStore.getState().reset());
afterEach(cleanup);

describe("PluginApplicationsView", () => {
  it("renders the macOS catalog hierarchy and opens an application", async () => {
    const adapter = port({ list: vi.fn(async () => [pluginApplicationFixture()]) });
    const onOpen = vi.fn();
    render(
      <PluginApplicationsView
        hostReady
        ownerUserId="alice"
        port={adapter}
        onOpen={onOpen}
        onOpenPluginManagement={() => undefined}
      />,
    );
    expect(await screen.findByRole("button", { name: /Canvas/ })).toBeVisible();
    expect(screen.getByText("本地服务")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /Canvas/ }));
    expect(onOpen).toHaveBeenCalledWith(pluginApplicationFixture());
  });

  it("opens plugin management from the empty state like macOS", async () => {
    const onOpenPluginManagement = vi.fn();
    render(
      <PluginApplicationsView
        hostReady
        ownerUserId="alice"
        port={port()}
        onOpen={() => undefined}
        onOpenPluginManagement={onOpenPluginManagement}
      />,
    );

    expect(await screen.findByRole("heading", { name: "还没有插件应用" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "打开插件管理" }));
    expect(onOpenPluginManagement).toHaveBeenCalledTimes(1);
  });

  it("offers shared and project contexts before launching", async () => {
    const adapter = port();
    render(
      <PluginApplicationHostView
        application={pluginApplicationFixture()}
        hostReady
        ownerUserId="alice"
        port={adapter}
        projects={[{ id: "project-1", title: "Project One", subtitle: null, conversationId: null }]}
        onBack={() => undefined}
      />,
    );
    expect(await screen.findByRole("heading", { name: "选择应用项目" })).toBeVisible();
    expect(screen.getByRole("button", { name: /公共项目/ })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: /Project One/ }));
    await waitFor(() => expect(adapter.launch).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      pluginId: "com.example.canvas",
      componentKey: "canvas-workbench",
      context: { kind: "project", projectId: "project-1" },
    })));
  });
});

function port(overrides: Partial<PluginApplicationPort> = {}): PluginApplicationPort {
  return {
    list: vi.fn(async () => []),
    launch: vi.fn(async () => pluginApplicationLaunchFixture()),
    discardPresentation: vi.fn(async () => undefined),
    handleBridgeRequest: vi.fn(async (request) => ({ requestId: request.requestId, result: {} })),
    ...overrides,
  };
}
