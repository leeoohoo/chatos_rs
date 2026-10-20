import { create } from "zustand";
import {
  canSubmitProject,
  normalizedProjectName,
  type CreatedLocalProject,
  type ProjectDirectorySelection,
  type ProjectTypeDefinition,
} from "../domain/projectCreation";
import type { ProjectCreationPort } from "../ports/ProjectCreationPort";

interface ProjectCreationState {
  ownerUserId: string | null;
  presentationId: string | null;
  generation: number;
  requestSequence: number;
  typeRequestId: number | null;
  pickerRequestId: number | null;
  saveRequestId: number | null;
  projectTypes: ProjectTypeDefinition[];
  selection: ProjectDirectorySelection | null;
  projectName: string;
  projectDescription: string;
  projectTypeKey: string;
  nameWasEdited: boolean;
  isLoadingTypes: boolean;
  isPicking: boolean;
  isSaving: boolean;
  errorMessage: string | null;
  open(port: ProjectCreationPort, ownerUserId: string): Promise<void>;
  close(port: ProjectCreationPort): void;
  pickDirectory(port: ProjectCreationPort): Promise<void>;
  setProjectName(value: string): void;
  setProjectDescription(value: string): void;
  setProjectTypeKey(value: string): void;
  save(port: ProjectCreationPort): Promise<CreatedLocalProject | null>;
}

let presentationSequence = 0;

export const useProjectCreationStore = create<ProjectCreationState>((set, get) => ({
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

  async open(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) return;
    const generation = get().generation + 1;
    const requestId = get().requestSequence + 1;
    const presentationId = `create-project-${Date.now()}-${++presentationSequence}`;
    set({
      ownerUserId,
      presentationId,
      generation,
      requestSequence: requestId,
      typeRequestId: requestId,
      pickerRequestId: null,
      saveRequestId: null,
      projectTypes: [],
      selection: null,
      projectName: "",
      projectDescription: "",
      projectTypeKey: "software_development",
      nameWasEdited: false,
      isLoadingTypes: true,
      isPicking: false,
      isSaving: false,
      errorMessage: null,
    });
    try {
      const projectTypes = await port.listProjectTypes();
      if (!accepts(get(), ownerUserId, presentationId, generation)
        || get().typeRequestId !== requestId) return;
      const projectTypeKey = projectTypes.some((item) => item.key === "software_development")
        ? "software_development"
        : projectTypes[0]?.key ?? "";
      set({ projectTypes, projectTypeKey, typeRequestId: null, isLoadingTypes: false });
    } catch (error) {
      if (!accepts(get(), ownerUserId, presentationId, generation)
        || get().typeRequestId !== requestId) return;
      set({ typeRequestId: null, isLoadingTypes: false, errorMessage: errorMessage(error) });
    }
  },

  close(port) {
    const { ownerUserId, presentationId } = get();
    set((state) => ({
      ownerUserId: null,
      presentationId: null,
      generation: state.generation + 1,
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
    }));
    if (ownerUserId && presentationId) {
      void port.discardProjectDirectorySelection(ownerUserId, presentationId).catch(() => undefined);
    }
  },

  async pickDirectory(port) {
    const current = get();
    if (!current.ownerUserId || !current.presentationId || current.isPicking || current.isSaving) return;
    const ownerUserId = current.ownerUserId;
    const presentationId = current.presentationId;
    const generation = current.generation;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      pickerRequestId: requestId,
      isPicking: true,
      errorMessage: null,
    });
    try {
      const selection = await port.pickProjectDirectory(ownerUserId, presentationId);
      if (!accepts(get(), ownerUserId, presentationId, generation)
        || get().pickerRequestId !== requestId) return;
      const update: Partial<ProjectCreationState> = {
        pickerRequestId: null,
        isPicking: false,
      };
      if (selection) {
        update.selection = selection;
        if (!get().nameWasEdited) update.projectName = selection.directoryName;
      }
      set(update);
    } catch (error) {
      if (!accepts(get(), ownerUserId, presentationId, generation)
        || get().pickerRequestId !== requestId) return;
      set({ pickerRequestId: null, isPicking: false, errorMessage: errorMessage(error) });
    }
  },

  setProjectName(value) {
    if (get().isSaving) return;
    set({ projectName: value, nameWasEdited: true, errorMessage: null });
  },

  setProjectDescription(value) {
    if (get().isSaving) return;
    set({ projectDescription: value, errorMessage: null });
  },

  setProjectTypeKey(value) {
    if (get().isSaving) return;
    set({ projectTypeKey: value, errorMessage: null });
  },

  async save(port) {
    const current = get();
    if (!current.ownerUserId || !current.presentationId || !current.selection
      || !canSubmitProject(current)) return null;
    const ownerUserId = current.ownerUserId;
    const presentationId = current.presentationId;
    const selectionId = current.selection.selectionId;
    const generation = current.generation;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      saveRequestId: requestId,
      isSaving: true,
      errorMessage: null,
    });
    try {
      const project = await port.createLocalProject({
        ownerUserId,
        presentationId,
        selectionId,
        name: normalizedProjectName(current.projectName),
        description: current.projectDescription.trim(),
        projectTypeKey: current.projectTypeKey,
      });
      if (!accepts(get(), ownerUserId, presentationId, generation)
        || get().saveRequestId !== requestId) return null;
      set({ saveRequestId: null, isSaving: false });
      return project;
    } catch (error) {
      if (!accepts(get(), ownerUserId, presentationId, generation)
        || get().saveRequestId !== requestId) return null;
      set({ saveRequestId: null, isSaving: false, errorMessage: errorMessage(error) });
      return null;
    }
  },
}));

function accepts(
  state: ProjectCreationState,
  ownerUserId: string,
  presentationId: string,
  generation: number,
): boolean {
  return state.ownerUserId === ownerUserId
    && state.presentationId === presentationId
    && state.generation === generation;
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "本地项目创建失败。";
}
