import { create } from "zustand";
import {
  applicationId,
  requiresPluginProjectContext,
  type PluginApplication,
  type PluginApplicationContextSelection,
  type PluginApplicationLaunch,
} from "../domain/pluginApplications";
import type { PluginApplicationPort } from "../ports/PluginApplicationPort";

export type PluginApplicationPhase =
  | "inactive"
  | "choosingContext"
  | "launching"
  | "ready"
  | "failed";

interface PluginApplicationState {
  ownerUserId: string | null;
  hostAvailable: boolean;
  accountGeneration: number;
  catalogActive: boolean;
  applications: PluginApplication[];
  isLoading: boolean;
  hasLoaded: boolean;
  catalogErrorMessage: string | null;
  requestSequence: number;
  activeCatalogRequest: number | null;
  activeApplication: PluginApplication | null;
  presentationId: string | null;
  presentationEpoch: number;
  launchGeneration: number;
  activeLaunchRequest: number | null;
  selectedContext: PluginApplicationContextSelection | null;
  phase: PluginApplicationPhase;
  launch: PluginApplicationLaunch | null;
  launchErrorMessage: string | null;
  activateCatalog(port: PluginApplicationPort, ownerUserId: string, hostAvailable: boolean): void;
  suspendCatalog(ownerUserId: string): void;
  refreshCatalog(port: PluginApplicationPort): Promise<void>;
  activateApplication(
    port: PluginApplicationPort,
    ownerUserId: string,
    hostAvailable: boolean,
    application: PluginApplication,
  ): void;
  selectContext(port: PluginApplicationPort, selection: PluginApplicationContextSelection): void;
  retry(port: PluginApplicationPort): void;
  chooseAnotherContext(port: PluginApplicationPort): void;
  deactivateApplication(
    port: PluginApplicationPort,
    ownerUserId: string,
    expectedApplicationId: string,
  ): void;
  reset(port?: PluginApplicationPort): void;
}

interface CatalogIdentity {
  ownerUserId: string;
  accountGeneration: number;
  requestId: number;
}

interface LaunchIdentity {
  ownerUserId: string;
  accountGeneration: number;
  presentationId: string;
  presentationEpoch: number;
  applicationId: string;
  generation: number;
  requestId: number;
}

let presentationSequence = 0;

const initialState = {
  ownerUserId: null,
  hostAvailable: false,
  accountGeneration: 0,
  catalogActive: false,
  applications: [],
  isLoading: false,
  hasLoaded: false,
  catalogErrorMessage: null,
  requestSequence: 0,
  activeCatalogRequest: null,
  activeApplication: null,
  presentationId: null,
  presentationEpoch: 0,
  launchGeneration: 0,
  activeLaunchRequest: null,
  selectedContext: null,
  phase: "inactive" as PluginApplicationPhase,
  launch: null,
  launchErrorMessage: null,
};

export const usePluginApplicationStore = create<PluginApplicationState>((set, get) => ({
  ...initialState,

  activateCatalog(port, ownerUserId, hostAvailable) {
    const current = get();
    const ownerChanged = current.ownerUserId !== ownerUserId;
    const hostChanged = current.hostAvailable !== hostAvailable;
    if (ownerChanged || hostChanged) {
      discardCurrentPresentation(port, current);
      set({
        ...initialState,
        ownerUserId,
        hostAvailable,
        accountGeneration: current.accountGeneration + 1,
        catalogActive: true,
        requestSequence: current.requestSequence + 1,
        presentationEpoch: current.presentationEpoch + 1,
        catalogErrorMessage: hostAvailable ? null : hostUnavailableMessage,
        hasLoaded: !hostAvailable,
      });
    } else {
      set({ catalogActive: true, hostAvailable });
    }
    const state = get();
    if (hostAvailable && (!state.hasLoaded || state.applications.length === 0 || ownerChanged)) {
      void loadCatalog(port, false, get, set);
    }
  },

  suspendCatalog(ownerUserId) {
    const state = get();
    if (state.ownerUserId !== ownerUserId || !state.catalogActive) return;
    set({
      catalogActive: false,
      isLoading: false,
      activeCatalogRequest: null,
      requestSequence: state.requestSequence + 1,
    });
  },

  async refreshCatalog(port) {
    await loadCatalog(port, true, get, set);
  },

  activateApplication(port, ownerUserId, hostAvailable, application) {
    const current = get();
    discardCurrentPresentation(port, current);
    const ownerChanged = current.ownerUserId !== ownerUserId;
    const accountGeneration = ownerChanged || current.hostAvailable !== hostAvailable
      ? current.accountGeneration + 1
      : current.accountGeneration;
    const presentationEpoch = current.presentationEpoch + 1;
    const presentationId = newPresentationId();
    const needsContext = requiresPluginProjectContext(application);
    set({
      ownerUserId,
      hostAvailable,
      accountGeneration,
      catalogActive: false,
      activeCatalogRequest: null,
      isLoading: false,
      requestSequence: current.requestSequence + 1,
      activeApplication: application,
      presentationId,
      presentationEpoch,
      launchGeneration: 0,
      activeLaunchRequest: null,
      selectedContext: null,
      phase: !hostAvailable ? "failed" : needsContext ? "choosingContext" : "inactive",
      launch: null,
      launchErrorMessage: hostAvailable ? null : hostUnavailableMessage,
      ...(ownerChanged ? { applications: [], hasLoaded: false, catalogErrorMessage: null } : {}),
    });
    if (hostAvailable && !needsContext) {
      launchApplication(port, { kind: "none" }, get, set);
    }
  },

  selectContext(port, selection) {
    const state = get();
    const application = state.activeApplication;
    if (!application || !state.hostAvailable || !requiresPluginProjectContext(application)) return;
    if (selection.kind === "device" && application.missingContext !== "device") return;
    if (selection.kind === "none") return;
    launchApplication(port, selection, get, set);
  },

  retry(port) {
    const state = get();
    if (!state.hostAvailable || !state.activeApplication || !state.selectedContext) return;
    launchApplication(port, state.selectedContext, get, set);
  },

  chooseAnotherContext(port) {
    const state = get();
    if (!state.activeApplication || !requiresPluginProjectContext(state.activeApplication)) return;
    discardCurrentPresentation(port, state);
    set({
      presentationEpoch: state.presentationEpoch + 1,
      activeLaunchRequest: null,
      requestSequence: state.requestSequence + 1,
      selectedContext: null,
      phase: "choosingContext",
      launch: null,
      launchErrorMessage: null,
    });
  },

  deactivateApplication(port, ownerUserId, expectedApplicationId) {
    const state = get();
    if (state.ownerUserId !== ownerUserId
      || !state.activeApplication
      || applicationId(state.activeApplication) !== expectedApplicationId) return;
    discardCurrentPresentation(port, state);
    set({
      activeApplication: null,
      presentationId: null,
      presentationEpoch: state.presentationEpoch + 1,
      launchGeneration: 0,
      activeLaunchRequest: null,
      selectedContext: null,
      phase: "inactive",
      launch: null,
      launchErrorMessage: null,
      requestSequence: state.requestSequence + 1,
    });
  },

  reset(port) {
    const state = get();
    if (port) discardCurrentPresentation(port, state);
    set({
      ...initialState,
      accountGeneration: state.accountGeneration + 1,
      requestSequence: state.requestSequence + 1,
      presentationEpoch: state.presentationEpoch + 1,
    });
  },
}));

async function loadCatalog(
  port: PluginApplicationPort,
  force: boolean,
  get: () => PluginApplicationState,
  set: (partial: Partial<PluginApplicationState>) => void,
) {
  const state = get();
  if (!state.catalogActive || !state.ownerUserId || !state.hostAvailable) {
    if (state.catalogActive && !state.hostAvailable) {
      set({ catalogErrorMessage: hostUnavailableMessage, hasLoaded: true, isLoading: false });
    }
    return;
  }
  if (!force && (state.activeCatalogRequest != null || state.hasLoaded)) return;
  const requestId = state.requestSequence + 1;
  const identity: CatalogIdentity = {
    ownerUserId: state.ownerUserId,
    accountGeneration: state.accountGeneration,
    requestId,
  };
  set({
    requestSequence: requestId,
    activeCatalogRequest: requestId,
    isLoading: true,
    catalogErrorMessage: null,
  });
  try {
    const applications = await port.list(identity.ownerUserId);
    if (!acceptsCatalog(get(), identity)) return;
    set({
      applications,
      isLoading: false,
      hasLoaded: true,
      activeCatalogRequest: null,
      catalogErrorMessage: null,
    });
  } catch (error) {
    if (!acceptsCatalog(get(), identity)) return;
    set({
      isLoading: false,
      hasLoaded: true,
      activeCatalogRequest: null,
      catalogErrorMessage: errorMessage(error, "插件应用暂时无法读取。"),
    });
  }
}

function launchApplication(
  port: PluginApplicationPort,
  selection: PluginApplicationContextSelection,
  get: () => PluginApplicationState,
  set: (partial: Partial<PluginApplicationState>) => void,
) {
  const state = get();
  const application = state.activeApplication;
  if (!application || !state.ownerUserId || !state.presentationId || !state.hostAvailable) return;
  const generation = state.launchGeneration + 1;
  const requestId = state.requestSequence + 1;
  const identity: LaunchIdentity = {
    ownerUserId: state.ownerUserId,
    accountGeneration: state.accountGeneration,
    presentationId: state.presentationId,
    presentationEpoch: state.presentationEpoch,
    applicationId: applicationId(application),
    generation,
    requestId,
  };
  set({
    requestSequence: requestId,
    launchGeneration: generation,
    activeLaunchRequest: requestId,
    selectedContext: selection,
    phase: "launching",
    launch: null,
    launchErrorMessage: null,
  });
  void port.launch({
    ownerUserId: identity.ownerUserId,
    presentationId: identity.presentationId,
    generation,
    pluginId: application.pluginId,
    componentKey: application.componentKey,
    context: selection,
  }).then((launch) => {
    if (!acceptsLaunch(get(), identity)) {
      void safeDiscard(port, identity.ownerUserId, identity.presentationId, generation);
      return;
    }
    if (applicationId(launch.application) !== identity.applicationId
      || launch.releaseId !== application.releaseId
      || launch.releaseDigest !== application.releaseDigest
      || (selection.kind === "project" && launch.context?.projectId !== selection.projectId)
      || (selection.kind !== "project" && launch.context != null)) {
      void safeDiscard(port, identity.ownerUserId, identity.presentationId, generation);
      set({
        activeLaunchRequest: null,
        phase: "failed",
        launchErrorMessage: "插件应用启动结果与当前展示上下文不一致。",
      });
      return;
    }
    set({
      activeLaunchRequest: null,
      phase: "ready",
      launch,
      launchErrorMessage: null,
    });
  }).catch((error) => {
    if (!acceptsLaunch(get(), identity)) return;
    set({
      activeLaunchRequest: null,
      phase: "failed",
      launch: null,
      launchErrorMessage: errorMessage(error, "插件应用暂时无法打开。"),
    });
  });
}

function acceptsCatalog(state: PluginApplicationState, identity: CatalogIdentity): boolean {
  return state.catalogActive
    && state.hostAvailable
    && state.ownerUserId === identity.ownerUserId
    && state.accountGeneration === identity.accountGeneration
    && state.activeCatalogRequest === identity.requestId;
}

function acceptsLaunch(state: PluginApplicationState, identity: LaunchIdentity): boolean {
  return state.hostAvailable
    && state.ownerUserId === identity.ownerUserId
    && state.accountGeneration === identity.accountGeneration
    && state.presentationId === identity.presentationId
    && state.presentationEpoch === identity.presentationEpoch
    && state.activeApplication != null
    && applicationId(state.activeApplication) === identity.applicationId
    && state.launchGeneration === identity.generation
    && state.activeLaunchRequest === identity.requestId;
}

function discardCurrentPresentation(port: PluginApplicationPort, state: PluginApplicationState) {
  if (!state.ownerUserId || !state.presentationId) return;
  void safeDiscard(
    port,
    state.ownerUserId,
    state.presentationId,
    Math.max(1, state.launchGeneration),
  );
}

async function safeDiscard(
  port: PluginApplicationPort,
  ownerUserId: string,
  presentationId: string,
  generation: number,
) {
  try {
    await port.discardPresentation({ ownerUserId, presentationId, generation });
  } catch {
    // Discard is best effort: the native generation tombstone and global reset are authoritative.
  }
}

function newPresentationId(): string {
  presentationSequence += 1;
  const random = globalThis.crypto?.randomUUID?.();
  return random
    ? `plugin-application-${random}`
    : `plugin-application-${Date.now().toString(36)}-${presentationSequence.toString(36)}`;
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}

const hostUnavailableMessage = "Local Agent Host 尚未就绪，请先恢复本机服务。";
