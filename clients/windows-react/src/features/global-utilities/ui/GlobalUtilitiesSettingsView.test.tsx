import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { useGlobalUtilityPreferencesStore } from "../application/globalUtilityPreferencesStore";
import type { GlobalUtilityPreferences } from "../domain/globalUtilities";
import type { GlobalUtilityPreferencesPort } from "../ports/GlobalUtilityPreferencesPort";
import { GlobalUtilitiesSettingsView } from "./GlobalUtilitiesSettingsView";

describe("GlobalUtilitiesSettingsView", () => {
  beforeEach(() => {
    useGeneralSettingsStore.getState().reset();
    useGlobalUtilityPreferencesStore.getState().reset();
  });
  afterEach(cleanup);

  it("requires the macOS conflict acknowledgement before enabling", async () => {
    const port = stubPort();
    await useGlobalUtilityPreferencesStore.getState().activate(port, "owner-1");
    render(<GlobalUtilitiesSettingsView ownerUserId="owner-1" port={port} />);
    fireEvent.click(screen.getByRole("checkbox", { name: "启用全局快捷工具" }));
    expect(screen.getByRole("dialog", { name: "快捷键冲突提示" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "启用" }));
    await waitFor(() => expect(port.update).toHaveBeenCalledWith("owner-1", expect.objectContaining({ isEnabled: true, hasAcknowledgedShortcutConflicts: true })));
  });

  it("records a modified shortcut and sends it through the native port", async () => {
    const port = stubPort();
    await useGlobalUtilityPreferencesStore.getState().activate(port, "owner-1");
    useGlobalUtilityPreferencesStore.getState().updateDraft({ isEnabled: true, hasAcknowledgedShortcutConflicts: true });
    await useGlobalUtilityPreferencesStore.getState().save(port);
    render(<GlobalUtilitiesSettingsView ownerUserId="owner-1" port={port} />);
    fireEvent.click(screen.getByRole("button", { name: "录制截屏快捷键" }));
    fireEvent.keyDown(window, { key: "k", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(port.update).toHaveBeenLastCalledWith("owner-1", expect.objectContaining({
      hotKeys: expect.objectContaining({ screenshot: { key: "K", modifiers: ["control", "shift"] } }),
    })));
  });
});

function stubPort(): GlobalUtilityPreferencesPort {
  return {
    load: vi.fn(async () => preferences()),
    update: vi.fn(async (_owner, value) => ({ ...value, revision: value.revision + 1 })),
    deactivate: vi.fn(async () => undefined),
  };
}

function preferences(): GlobalUtilityPreferences {
  const hotKeys = {
    screenshot: { key: "A", modifiers: ["control" as const] },
    screenRecording: { key: "Q", modifiers: ["control" as const] },
    clipboardHistory: { key: "E", modifiers: ["control" as const] },
    quickSearch: { key: "Space", modifiers: ["control" as const] },
  };
  const registration = (action: keyof typeof hotKeys) => ({ state: "disabled" as const, requested: hotKeys[action], active: null, usesFallback: false });
  return {
    isEnabled: false, hasAcknowledgedShortcutConflicts: false,
    screenshotEnabled: true, screenRecordingEnabled: true, clipboardHistoryEnabled: true, quickSearchEnabled: true,
    hotKeys,
    registrations: {
      screenshot: registration("screenshot"), screenRecording: registration("screenRecording"),
      clipboardHistory: registration("clipboardHistory"), quickSearch: registration("quickSearch"),
    },
    revision: 1,
  };
}
