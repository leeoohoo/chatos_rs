import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { usePetPreferencesStore } from "../application/petPreferencesStore";
import type { PetPreferencesPort } from "../ports/PetPreferencesPort";
import { PetSettingsView } from "./PetSettingsView";

describe("PetSettingsView", () => {
  beforeEach(() => {
    useGeneralSettingsStore.getState().reset();
    usePetPreferencesStore.getState().reset();
  });
  afterEach(cleanup);

  it("renders the macOS-derived cards and version-saves a toggle", async () => {
    const port = stubPort();
    await usePetPreferencesStore.getState().activate(port, "owner-1");
    render(<PetSettingsView ownerUserId="owner-1" port={port} />);
    expect(screen.getByRole("heading", { name: "宠物" })).toBeVisible();
    expect(screen.getByText("枫团 · 秋日小狐狸")).toBeVisible();
    fireEvent.click(screen.getByRole("checkbox", { name: "启用全局宠物" }));
    await waitFor(() => expect(port.update).toHaveBeenCalledWith("owner-1", expect.objectContaining({ isEnabled: false })));
  });

  it("asks Rust to restore the trusted default position", async () => {
    const port = stubPort();
    await usePetPreferencesStore.getState().activate(port, "owner-1");
    render(<PetSettingsView ownerUserId="owner-1" port={port} />);
    fireEvent.click(screen.getByRole("button", { name: "恢复默认位置" }));
    await waitFor(() => expect(port.resetPosition).toHaveBeenCalledWith("owner-1", 1));
  });
});

function stubPort(): PetPreferencesPort {
  const snapshot = { isEnabled: true, size: 104, showProcess: true, showCompletions: true, showAcrossSpaces: true, revision: 1 };
  return {
    load: vi.fn(async () => snapshot),
    update: vi.fn(async (_owner, value) => ({ ...value, revision: value.revision + 1 })),
    resetPosition: vi.fn(async (_owner, revision) => ({ ...snapshot, revision: revision + 1 })),
  };
}
