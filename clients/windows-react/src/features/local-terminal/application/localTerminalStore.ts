import { create } from "zustand";
import {
  localTerminalTitleForDirectory,
  localTerminalTitleFromShell,
  type LocalTerminalEvent,
} from "../domain/localTerminal";
import type { LocalTerminalPort, LocalTerminalSessionScope } from "../ports/LocalTerminalPort";

export type LocalTerminalPhase = "starting" | "running" | "exited" | "failed" | "closed";
type LocalTerminalAction = "interrupt";

export interface LocalTerminalTabState {
  id: string;
  presentationId: string;
  sessionId: string | null;
  generation: number;
  startRequestId: number;
  phase: LocalTerminalPhase;
  title: string;
  shellName: string;
  workingDirectory: string;
  columns: number;
  rows: number;
  cursor: number;
  events: LocalTerminalEvent[];
  retainedOutputBytes: number;
  outputGeneration: number;
  exitCode: number | null;
  activeAction: LocalTerminalAction | null;
  errorMessage: string | null;
  warningMessage: string | null;
}

interface LocalTerminalWorkspaceState {
  ownerUserId: string | null;
  hostAvailable: boolean;
  generation: number;
  tabs: LocalTerminalTabState[];
  selectedTabId: string | null;
  activate(port: LocalTerminalPort, ownerUserId: string, hostAvailable: boolean): Promise<void>;
  deactivate(port: LocalTerminalPort): void;
  createTerminal(port: LocalTerminalPort): void;
  closeTerminal(port: LocalTerminalPort, tabId: string): void;
  selectTerminal(tabId: string): void;
  interrupt(port: LocalTerminalPort, tabId: string): Promise<void>;
  sendInput(port: LocalTerminalPort, tabId: string, data: Uint8Array): void;
  resize(port: LocalTerminalPort, tabId: string, columns: number, rows: number): Promise<void>;
  updateTitle(tabId: string, value: string): void;
  updateWorkingDirectory(tabId: string, value: string): void;
  clearOutput(tabId: string): void;
  dismissFeedback(tabId: string): void;
}

interface WorkspaceContext {
  ownerUserId: string;
  generation: number;
}

interface SessionContext extends WorkspaceContext {
  tabId: string;
  presentationId: string;
  sessionId: string;
  tabGeneration: number;
}

const maximumRetainedEvents = 512;
const maximumRetainedOutputBytes = 2 * 1_024 * 1_024;
let tabSequence = 0;
let lifecycleBarrier: Promise<void> = Promise.resolve();
const inputBarriers = new Map<string, Promise<void>>();

export const useLocalTerminalStore = create<LocalTerminalWorkspaceState>((set, get) => ({
  ownerUserId: null,
  hostAvailable: false,
  generation: 0,
  tabs: [],
  selectedTabId: null,

  async activate(port, ownerUserId, hostAvailable) {
    const current = get();
    if (current.ownerUserId === ownerUserId && current.hostAvailable === hostAvailable) {
      if (hostAvailable && current.tabs.length === 0) current.createTerminal(port);
      return;
    }
    const stale = current.tabs.map((tab) => ({
      ownerUserId: current.ownerUserId,
      presentationId: tab.presentationId,
      sessionId: tab.sessionId,
    }));
    const generation = current.generation + 1;
    set({
      ownerUserId,
      hostAvailable,
      generation,
      tabs: [],
      selectedTabId: null,
    });
    await queueCleanup(port, stale);
    const state = get();
    if (state.ownerUserId !== ownerUserId || state.generation !== generation || !state.hostAvailable) return;
    state.createTerminal(port);
  },

  deactivate(port) {
    const state = get();
    const stale = state.tabs.map((tab) => ({
      ownerUserId: state.ownerUserId,
      presentationId: tab.presentationId,
      sessionId: tab.sessionId,
    }));
    set({
      ownerUserId: null,
      hostAvailable: false,
      generation: state.generation + 1,
      tabs: [],
      selectedTabId: null,
    });
    void queueCleanup(port, stale);
  },

  createTerminal(port) {
    const state = get();
    if (!state.ownerUserId || !state.hostAvailable) return;
    tabSequence += 1;
    const id = `terminal-${tabSequence}`;
    const presentationId = `local-terminal-tab-${tabSequence}-${Date.now().toString(36)}`;
    const tab: LocalTerminalTabState = {
      id,
      presentationId,
      sessionId: null,
      generation: 1,
      startRequestId: 1,
      phase: "starting",
      title: "终端",
      shellName: "PowerShell",
      workingDirectory: "准备中…",
      columns: 80,
      rows: 24,
      cursor: 0,
      events: [],
      retainedOutputBytes: 0,
      outputGeneration: 0,
      exitCode: null,
      activeAction: null,
      errorMessage: null,
      warningMessage: null,
    };
    const context: WorkspaceContext = {
      ownerUserId: state.ownerUserId,
      generation: state.generation,
    };
    set({ tabs: [...state.tabs, tab], selectedTabId: id });
    void startTerminal(port, context, tab);
  },

  closeTerminal(port, tabId) {
    const state = get();
    const index = state.tabs.findIndex((tab) => tab.id === tabId);
    if (index < 0) return;
    const tab = state.tabs[index];
    const remaining = state.tabs.filter((candidate) => candidate.id !== tabId);
    const nextSelected = state.selectedTabId === tabId
      ? remaining[Math.min(index, remaining.length - 1)]?.id ?? null
      : state.selectedTabId;
    set({ tabs: remaining, selectedTabId: nextSelected });
    void queueCleanup(port, [{
      ownerUserId: state.ownerUserId,
      presentationId: tab.presentationId,
      sessionId: tab.sessionId,
    }]);
    if (remaining.length === 0 && state.ownerUserId && state.hostAvailable) {
      queueMicrotask(() => get().createTerminal(port));
    }
  },

  selectTerminal(tabId) {
    if (get().tabs.some((tab) => tab.id === tabId)) set({ selectedTabId: tabId });
  },

  async interrupt(port, tabId) {
    const context = sessionContext(get(), tabId);
    if (!context) return;
    updateTab(set, tabId, (tab) => ({ ...tab, activeAction: "interrupt", errorMessage: null }));
    try {
      await port.interrupt(scope(context));
      if (acceptsSession(get(), context)) {
        updateTab(set, tabId, (tab) => ({ ...tab, activeAction: null }));
      }
    } catch (error) {
      if (acceptsSession(get(), context)) {
        updateTab(set, tabId, (tab) => ({
          ...tab,
          activeAction: null,
          errorMessage: errorMessage(error),
        }));
      }
    }
  },

  sendInput(port, tabId, data) {
    if (data.length === 0 || data.length > 64 * 1_024) return;
    const context = sessionContext(get(), tabId);
    if (!context) return;
    const key = `${context.ownerUserId}:${context.sessionId}`;
    const previous = inputBarriers.get(key) ?? Promise.resolve();
    const next = previous
      .catch(() => undefined)
      .then(async () => {
        if (!acceptsSession(get(), context)) return;
        await port.sendInput({ ...scope(context), data });
      })
      .catch((error) => {
        if (acceptsSession(get(), context)) {
          updateTab(set, tabId, (tab) => ({ ...tab, errorMessage: errorMessage(error) }));
        }
      })
      .finally(() => {
        if (inputBarriers.get(key) === next) inputBarriers.delete(key);
      });
    inputBarriers.set(key, next);
  },

  async resize(port, tabId, columns, rows) {
    const context = sessionContext(get(), tabId);
    if (!context) return;
    const nextColumns = boundedDimension(columns);
    const nextRows = boundedDimension(rows);
    updateTab(set, tabId, (tab) => ({ ...tab, columns: nextColumns, rows: nextRows }));
    try {
      await port.resize({ ...scope(context), columns: nextColumns, rows: nextRows });
    } catch (error) {
      if (acceptsSession(get(), context)) {
        updateTab(set, tabId, (tab) => ({ ...tab, errorMessage: errorMessage(error) }));
      }
    }
  },

  updateTitle(tabId, value) {
    updateTab(set, tabId, (tab) => {
      const title = localTerminalTitleFromShell(value, tab.workingDirectory);
      return title ? { ...tab, title } : tab;
    });
  },

  updateWorkingDirectory(tabId, value) {
    const directory = value.trim();
    if (directory) updateTab(set, tabId, (tab) => ({ ...tab, workingDirectory: directory }));
  },

  clearOutput(tabId) {
    updateTab(set, tabId, (tab) => ({
      ...tab,
      events: [],
      retainedOutputBytes: 0,
      outputGeneration: tab.outputGeneration + 1,
    }));
  },

  dismissFeedback(tabId) {
    updateTab(set, tabId, (tab) => ({ ...tab, errorMessage: null, warningMessage: null }));
  },
}));

async function startTerminal(
  port: LocalTerminalPort,
  workspace: WorkspaceContext,
  startingTab: LocalTerminalTabState,
) {
  try {
    await lifecycleBarrier.catch(() => undefined);
    if (!acceptsStart(useLocalTerminalStore.getState(), workspace, startingTab)) return;
    const response = await port.start({
      ownerUserId: workspace.ownerUserId,
      presentationId: startingTab.presentationId,
      columns: startingTab.columns,
      rows: startingTab.rows,
    });
    const state = useLocalTerminalStore.getState();
    const current = state.tabs.find((tab) => tab.id === startingTab.id);
    if (!acceptsStart(state, workspace, startingTab) || !current) {
      await safeDiscard(port, workspace.ownerUserId, startingTab.presentationId);
      return;
    }
    const session: SessionContext = {
      ...workspace,
      tabId: startingTab.id,
      presentationId: startingTab.presentationId,
      sessionId: response.sessionId,
      tabGeneration: startingTab.generation,
    };
    updateTab(useLocalTerminalStore.setState, startingTab.id, (tab) => ({
      ...tab,
      sessionId: response.sessionId,
      phase: "running",
      shellName: response.shellName,
      workingDirectory: response.initialWorkingDirectory,
      title: localTerminalTitleForDirectory(response.initialWorkingDirectory),
    }));
    void pollTerminal(port, session);
  } catch (error) {
    const state = useLocalTerminalStore.getState();
    if (!acceptsStart(state, workspace, startingTab)) return;
    updateTab(useLocalTerminalStore.setState, startingTab.id, (tab) => ({
      ...tab,
      phase: "failed",
      errorMessage: errorMessage(error),
    }));
    await safeDiscard(port, workspace.ownerUserId, startingTab.presentationId);
  }
}

async function pollTerminal(port: LocalTerminalPort, context: SessionContext): Promise<void> {
  while (acceptsSession(useLocalTerminalStore.getState(), context)) {
    const current = useLocalTerminalStore.getState().tabs.find((tab) => tab.id === context.tabId);
    if (!current) return;
    try {
      const poll = await port.poll({
        ...scope(context),
        afterCursor: current.cursor,
        timeoutMs: 30_000,
      });
      if (!acceptsSession(useLocalTerminalStore.getState(), context)) return;
      if (poll.sessionId !== context.sessionId) throw new Error("本机终端返回了不匹配的会话。 ");
      updateTab(useLocalTerminalStore.setState, context.tabId, (tab) => {
        const freshEvents = poll.events.filter((event) => event.cursor > tab.cursor);
        const retained = retainEvents(tab.events, freshEvents);
        return {
          ...tab,
          events: retained.events,
          retainedOutputBytes: retained.outputBytes,
          cursor: Math.max(tab.cursor, poll.nextCursor),
          exitCode: poll.exitCode,
          warningMessage: poll.truncated
            ? "部分较早的终端输出已从安全缓冲区移除。"
            : tab.warningMessage,
          phase: poll.running ? tab.phase : "exited",
          sessionId: poll.running ? tab.sessionId : null,
        };
      });
      if (!poll.running) {
        await safeClose(port, context);
        return;
      }
    } catch (error) {
      if (!acceptsSession(useLocalTerminalStore.getState(), context)) return;
      updateTab(useLocalTerminalStore.setState, context.tabId, (tab) => ({
        ...tab,
        phase: "failed",
        sessionId: null,
        errorMessage: errorMessage(error),
      }));
      await safeClose(port, context);
      await safeDiscard(port, context.ownerUserId, context.presentationId);
      return;
    }
  }
}

function sessionContext(state: LocalTerminalWorkspaceState, tabId: string): SessionContext | null {
  const tab = state.tabs.find((candidate) => candidate.id === tabId);
  if (!state.ownerUserId || !tab?.sessionId || tab.phase !== "running") return null;
  return {
    ownerUserId: state.ownerUserId,
    generation: state.generation,
    tabId,
    presentationId: tab.presentationId,
    sessionId: tab.sessionId,
    tabGeneration: tab.generation,
  };
}

function acceptsStart(
  state: LocalTerminalWorkspaceState,
  workspace: WorkspaceContext,
  startingTab: LocalTerminalTabState,
) {
  const tab = state.tabs.find((candidate) => candidate.id === startingTab.id);
  return state.ownerUserId === workspace.ownerUserId
    && state.generation === workspace.generation
    && state.hostAvailable
    && tab?.presentationId === startingTab.presentationId
    && tab.generation === startingTab.generation
    && tab.startRequestId === startingTab.startRequestId
    && tab.phase === "starting";
}

function acceptsSession(state: LocalTerminalWorkspaceState, context: SessionContext) {
  const tab = state.tabs.find((candidate) => candidate.id === context.tabId);
  return state.ownerUserId === context.ownerUserId
    && state.generation === context.generation
    && state.hostAvailable
    && tab?.presentationId === context.presentationId
    && tab.sessionId === context.sessionId
    && tab.generation === context.tabGeneration
    && tab.phase === "running";
}

function scope(context: SessionContext): LocalTerminalSessionScope {
  return {
    ownerUserId: context.ownerUserId,
    presentationId: context.presentationId,
    sessionId: context.sessionId,
  };
}

function queueCleanup(
  port: LocalTerminalPort,
  stale: Array<{
    ownerUserId: string | null;
    presentationId: string;
    sessionId: string | null;
  }>,
) {
  lifecycleBarrier = lifecycleBarrier
    .catch(() => undefined)
    .then(async () => {
      for (const item of stale) {
        if (!item.ownerUserId) continue;
        if (item.sessionId) {
          await safeClose(port, {
            ownerUserId: item.ownerUserId,
            presentationId: item.presentationId,
            sessionId: item.sessionId,
          });
        }
        await safeDiscard(port, item.ownerUserId, item.presentationId);
      }
    });
  return lifecycleBarrier;
}

async function safeClose(
  port: LocalTerminalPort,
  context: LocalTerminalSessionScope,
) {
  try { await port.close(context); } catch { /* teardown is idempotent */ }
}

async function safeDiscard(port: LocalTerminalPort, ownerUserId: string, presentationId: string) {
  try { await port.discardPresentation({ ownerUserId, presentationId }); } catch { /* native reset is the final barrier */ }
}

function retainEvents(existing: LocalTerminalEvent[], incoming: LocalTerminalEvent[]) {
  const events = [...existing, ...incoming];
  let outputBytes = events.reduce(
    (total, event) => total + (event.kind === "output" ? event.data.byteLength : 0),
    0,
  );
  while (events.length > maximumRetainedEvents || outputBytes > maximumRetainedOutputBytes) {
    const removed = events.shift();
    if (removed?.kind === "output") outputBytes -= removed.data.byteLength;
  }
  return { events, outputBytes };
}

function updateTab(
  set: typeof useLocalTerminalStore.setState,
  tabId: string,
  update: (tab: LocalTerminalTabState) => LocalTerminalTabState,
) {
  set((state) => ({
    tabs: state.tabs.map((tab) => tab.id === tabId ? update(tab) : tab),
  }));
}

function boundedDimension(value: number) {
  return Math.min(1_000, Math.max(1, Math.trunc(value) || 1));
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message.trim();
  if (typeof error === "string" && error.trim()) return error.trim();
  return "本机终端通信失败。";
}
