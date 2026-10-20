import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CreatedLocalProject } from "../domain/projectCreation";
import type { ProjectCreationPort } from "../ports/ProjectCreationPort";
import { useProjectCreationStore } from "./projectCreationStore";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accepted) => { resolve = accepted; });
  return { promise, resolve };
}

function port(overrides: Partial<ProjectCreationPort> = {}): ProjectCreationPort {
  return {
    listProjectTypes: vi.fn().mockResolvedValue([{
      key: "software_development",
      label: "通用软件开发",
      description: "通用项目",
      categoryKey: "software_product",
      categoryLabel: "软件与数字产品",
    }]),
    pickProjectDirectory: vi.fn().mockResolvedValue({
      selectionId: "selection-1",
      directoryName: "repo",
    }),
    discardProjectDirectorySelection: vi.fn().mockResolvedValue(undefined),
    createLocalProject: vi.fn().mockResolvedValue({
      id: "project-1",
      name: "repo",
      revision: 1,
      projectTypeKey: "software_development",
    }),
    ...overrides,
  };
}

describe("project creation store", () => {
  beforeEach(() => {
    useProjectCreationStore.setState({
      ownerUserId: null,
      presentationId: null,
      generation: 0,
      requestSequence: 0,
      typeRequestId: null,
      pickerRequestId: null,
      saveRequestId: null,
      projectTypes: [],
      selection: null,
      projectName: "",
      projectDescription: "",
      projectTypeKey: "software_development",
      nameWasEdited: false,
      isLoadingTypes: false,
      isPicking: false,
      isSaving: false,
      errorMessage: null,
    });
  });

  it("derives the untouched project name from the native directory display name", async () => {
    const adapter = port();
    await useProjectCreationStore.getState().open(adapter, "alice");
    await useProjectCreationStore.getState().pickDirectory(adapter);
    expect(useProjectCreationStore.getState().projectName).toBe("repo");
    useProjectCreationStore.getState().setProjectName("Custom");
    await useProjectCreationStore.getState().pickDirectory(adapter);
    expect(useProjectCreationStore.getState().projectName).toBe("Custom");
  });

  it("rejects picker results after the sheet closes and discards the native ticket", async () => {
    const picker = deferred<{ selectionId: string; directoryName: string } | null>();
    const adapter = port({ pickProjectDirectory: vi.fn(() => picker.promise) });
    await useProjectCreationStore.getState().open(adapter, "alice");
    const presentationId = useProjectCreationStore.getState().presentationId!;
    const selecting = useProjectCreationStore.getState().pickDirectory(adapter);
    useProjectCreationStore.getState().close(adapter);
    picker.resolve({ selectionId: "late", directoryName: "late-folder" });
    await selecting;
    expect(useProjectCreationStore.getState().selection).toBeNull();
    expect(adapter.discardProjectDirectorySelection).toHaveBeenCalledWith("alice", presentationId);
  });

  it("rejects an old owner save after a new sheet replaces it", async () => {
    const save = deferred<CreatedLocalProject>();
    const adapter = port({ createLocalProject: vi.fn(() => save.promise) });
    await useProjectCreationStore.getState().open(adapter, "alice");
    await useProjectCreationStore.getState().pickDirectory(adapter);
    const saving = useProjectCreationStore.getState().save(adapter);
    await useProjectCreationStore.getState().open(adapter, "bob");
    save.resolve({ id: "alice-project", name: "Alice", revision: 1, projectTypeKey: "software_development" });
    expect(await saving).toBeNull();
    expect(useProjectCreationStore.getState().ownerUserId).toBe("bob");
  });

  it("submits only the opaque selection identity and normalized metadata", async () => {
    const adapter = port();
    await useProjectCreationStore.getState().open(adapter, "alice");
    await useProjectCreationStore.getState().pickDirectory(adapter);
    useProjectCreationStore.getState().setProjectName("  Local  ");
    useProjectCreationStore.getState().setProjectDescription("  Notes  ");
    expect(await useProjectCreationStore.getState().save(adapter)).toMatchObject({ id: "project-1" });
    expect(adapter.createLocalProject).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      selectionId: "selection-1",
      name: "Local",
      description: "Notes",
    }));
    expect(JSON.stringify(vi.mocked(adapter.createLocalProject).mock.calls)).not.toContain("absolute");
  });
});
