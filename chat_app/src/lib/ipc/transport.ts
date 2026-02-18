// Lightweight IPC transport for Electron integration.
// If window.chatAPI is available (injected by preload), we use it to start/stop
// chat streams and wrap IPC events into a ReadableStream that mimics SSE.

export function ipcAvailable(): boolean {
  try {
    return typeof window !== 'undefined'
      && !!(window as any).chatAPI
      && typeof (window as any).chatAPI.startStream === 'function'
      && typeof (window as any).chatAPI.onStream === 'function'
      && typeof (window as any).chatAPI.stopStream === 'function';
  } catch (_) {
    return false;
  }
}

function toSseLine(evt: any): string {
  // Normalize several common shapes to SSE payload the frontend expects
  // Expected shapes downstream:
  //  - chunk:   { type: 'chunk', content: '...' }
  //  - thinking:{ type: 'thinking', content: '...' }
  //  - tools_start/tools_stream/tools_end: { type: 'tools_*', data: {...} }
  //  - complete: { type: 'complete', result: {...} }
  //  - cancelled: { type: 'cancelled' }
  //  - error: { type: 'error', data: { error } }
  try {
    if (!evt || typeof evt !== 'object') {
      return `data: ${JSON.stringify({ type: 'error', data: { error: 'invalid_event' } })}\n\n`;
    }
    const type = evt.type || 'chunk';
    let payload: any = { type };
    if (evt.content !== undefined) payload.content = evt.content;
    if (evt.data !== undefined) payload.data = evt.data;
    if (evt.result !== undefined) payload.result = evt.result;
    if (evt.error !== undefined) payload = { type: 'error', data: { error: String(evt.error) } };
    return `data: ${JSON.stringify(payload)}\n\n`;
  } catch (_) {
    return `data: ${JSON.stringify({ type: 'error', data: { error: 'serialize_failed' } })}\n\n`;
  }
}

function toReadableStreamFromIPC(sessionId: string, startArgs: any): ReadableStream<Uint8Array> {
  const chatAPI = (window as any).chatAPI;
  const encoder = new TextEncoder();
  return new ReadableStream<Uint8Array>({
    start(controller) {
      let off: (() => void) | null = null;
      try {
        off = chatAPI.onStream(sessionId, (evt: any) => {
          try {
            // push SSE-formatted line
            const line = toSseLine(evt);
            controller.enqueue(encoder.encode(line));
            const t = (evt && evt.type) || '';
            if (t === 'complete' || t === 'cancelled' || t === 'error') {
              controller.enqueue(encoder.encode('data: [DONE]\n\n'));
              if (off) off();
              controller.close();
            }
          } catch (_) {
            // ensure stream closes on fatal error
            controller.enqueue(encoder.encode(`data: ${JSON.stringify({ type: 'error', data: { error: 'ipc_stream_failed' } })}\n\n`));
            controller.enqueue(encoder.encode('data: [DONE]\n\n'));
            if (off) off();
            controller.close();
          }
        });

        Promise.resolve(chatAPI.startStream(startArgs)).catch((err: any) => {
          controller.enqueue(encoder.encode(`data: ${JSON.stringify({ type: 'error', data: { error: String(err?.message || err) } })}\n\n`));
          controller.enqueue(encoder.encode('data: [DONE]\n\n'));
          if (off) off();
          controller.close();
        });
      } catch (err: any) {
        controller.enqueue(encoder.encode(`data: ${JSON.stringify({ type: 'error', data: { error: String(err?.message || err) } })}\n\n`));
        controller.enqueue(encoder.encode('data: [DONE]\n\n'));
        if (off) off();
        controller.close();
      }
    },
    cancel() {
      try { /* consumer cancelled */ } catch (_) {}
    }
  });
}

export function streamChatIPC(
  sessionId: string,
  content: string,
  modelConfig: any,
  userId?: string,
  attachments?: any[],
  reasoningEnabled?: boolean,
  turnId?: string
): ReadableStream<Uint8Array> {
  const arg = {
    session_id: sessionId,
    content,
    user_id: userId,
    attachments: attachments || [],
    reasoning_enabled: reasoningEnabled,
    turn_id: turnId,
    ai_model_config: {
      provider: modelConfig?.provider,
      model_name: modelConfig?.model_name,
      temperature: modelConfig?.temperature ?? 0.7,
      use_tools: true,
      thinking_level: modelConfig?.thinking_level,
      api_key: modelConfig?.api_key,
      base_url: modelConfig?.base_url,
      supports_images: modelConfig?.supports_images === true,
      supports_reasoning: modelConfig?.supports_reasoning === true,
      supports_responses: modelConfig?.supports_responses === true
    }
  };
  return toReadableStreamFromIPC(sessionId, arg);
}

export function streamAgentChatIPC(
  sessionId: string,
  content: string,
  agentId: string,
  userId?: string,
  attachments?: any[],
  reasoningEnabled?: boolean,
  options?: { useResponses?: boolean; turnId?: string }
): ReadableStream<Uint8Array> {
  const arg = {
    session_id: sessionId,
    content,
    agent_id: agentId,
    user_id: userId,
    attachments: attachments || [],
    reasoning_enabled: reasoningEnabled,
    turn_id: options?.turnId,
    use_responses: options?.useResponses === true
  };
  return toReadableStreamFromIPC(sessionId, arg);
}

export async function stopChatIPC(sessionId: string): Promise<void> {
  try {
    const chatAPI = (window as any).chatAPI;
    if (chatAPI && typeof chatAPI.stopStream === 'function') {
      await chatAPI.stopStream(sessionId);
    }
  } catch (_) {}
}

// ----- Optional IPC helpers for REST-like endpoints -----
export async function listSessionsIPC(userId?: string, projectId?: string) {
  const api = (window as any).chatAPI;
  if (api?.listSessions) return api.listSessions({ user_id: userId, project_id: projectId });
  throw new Error('IPC listSessions not available');
}

export async function createSessionIPC(data: { id?: string; title: string; user_id: string; project_id?: string }) {
  const api = (window as any).chatAPI;
  if (api?.createSession) return api.createSession(data);
  throw new Error('IPC createSession not available');
}

export async function getSessionIPC(id: string) {
  const api = (window as any).chatAPI;
  if (api?.getSession) return api.getSession(id);
  throw new Error('IPC getSession not available');
}

export async function deleteSessionIPC(id: string) {
  const api = (window as any).chatAPI;
  if (api?.deleteSession) return api.deleteSession(id);
  throw new Error('IPC deleteSession not available');
}

export async function getSessionMessagesIPC(sessionId: string, opts?: { limit?: number; offset?: number }) {
  const api = (window as any).chatAPI;
  if (api?.getSessionMessages) return api.getSessionMessages(sessionId, opts || {});
  throw new Error('IPC getSessionMessages not available');
}

export async function createMessageIPC(payload: any) {
  const api = (window as any).chatAPI;
  if (api?.createMessage) return api.createMessage(payload);
  throw new Error('IPC createMessage not available');
}

export async function getUserSettingsIPC(userId?: string) {
  const api = (window as any).chatAPI;
  if (api?.getUserSettings) return api.getUserSettings(userId);
  throw new Error('IPC getUserSettings not available');
}

export async function updateUserSettingsIPC(userId: string, settings: Record<string, any>) {
  const api = (window as any).chatAPI;
  if (api?.updateUserSettings) return api.updateUserSettings(userId, settings);
  throw new Error('IPC updateUserSettings not available');
}

// ----- MCP configs -----
export async function getMcpConfigsIPC(userId?: string) {
  const api = (window as any).chatAPI; if (api?.getMcpConfigs) return api.getMcpConfigs(userId); throw new Error('IPC getMcpConfigs not available');
}
export async function createMcpConfigIPC(data: any) {
  const api = (window as any).chatAPI; if (api?.createMcpConfig) return api.createMcpConfig(data); throw new Error('IPC createMcpConfig not available');
}
export async function updateMcpConfigIPC(id: string, data: any) {
  const api = (window as any).chatAPI; if (api?.updateMcpConfig) return api.updateMcpConfig(id, data); throw new Error('IPC updateMcpConfig not available');
}
export async function deleteMcpConfigIPC(id: string) {
  const api = (window as any).chatAPI; if (api?.deleteMcpConfig) return api.deleteMcpConfig(id); throw new Error('IPC deleteMcpConfig not available');
}
export async function getMcpConfigResourceIPC(configId: string) {
  const api = (window as any).chatAPI; if (api?.getMcpConfigResource) return api.getMcpConfigResource(configId); throw new Error('IPC getMcpConfigResource not available');
}
export async function getMcpConfigResourceByCommandIPC(data: any) {
  const api = (window as any).chatAPI; if (api?.getMcpConfigResourceByCommand) return api.getMcpConfigResourceByCommand(data); throw new Error('IPC getMcpConfigResourceByCommand not available');
}

// ----- AI model configs -----
export async function getAiModelConfigsIPC(userId?: string) {
  const api = (window as any).chatAPI; if (api?.getAiModelConfigs) return api.getAiModelConfigs(userId); throw new Error('IPC getAiModelConfigs not available');
}
export async function createAiModelConfigIPC(data: any) {
  const api = (window as any).chatAPI; if (api?.createAiModelConfig) return api.createAiModelConfig(data); throw new Error('IPC createAiModelConfig not available');
}
export async function updateAiModelConfigIPC(id: string, data: any) {
  const api = (window as any).chatAPI; if (api?.updateAiModelConfig) return api.updateAiModelConfig(id, data); throw new Error('IPC updateAiModelConfig not available');
}
export async function deleteAiModelConfigIPC(id: string) {
  const api = (window as any).chatAPI; if (api?.deleteAiModelConfig) return api.deleteAiModelConfig(id); throw new Error('IPC deleteAiModelConfig not available');
}

// ----- System contexts -----
export async function getSystemContextsIPC(userId: string) {
  const api = (window as any).chatAPI; if (api?.getSystemContexts) return api.getSystemContexts(userId); throw new Error('IPC getSystemContexts not available');
}
export async function getActiveSystemContextIPC(userId: string) {
  const api = (window as any).chatAPI; if (api?.getActiveSystemContext) return api.getActiveSystemContext(userId); throw new Error('IPC getActiveSystemContext not available');
}
export async function createSystemContextIPC(data: any) {
  const api = (window as any).chatAPI; if (api?.createSystemContext) return api.createSystemContext(data); throw new Error('IPC createSystemContext not available');
}
export async function updateSystemContextIPC(id: string, data: any) {
  const api = (window as any).chatAPI; if (api?.updateSystemContext) return api.updateSystemContext(id, data); throw new Error('IPC updateSystemContext not available');
}
export async function deleteSystemContextIPC(id: string) {
  const api = (window as any).chatAPI; if (api?.deleteSystemContext) return api.deleteSystemContext(id); throw new Error('IPC deleteSystemContext not available');
}
export async function activateSystemContextIPC(id: string, userId: string) {
  const api = (window as any).chatAPI; if (api?.activateSystemContext) return api.activateSystemContext(id, userId); throw new Error('IPC activateSystemContext not available');
}

// ----- Agents -----
export async function getAgentsIPC(userId?: string) {
  const api = (window as any).chatAPI; if (api?.getAgents) return api.getAgents(userId); throw new Error('IPC getAgents not available');
}
export async function createAgentIPC(data: any) {
  const api = (window as any).chatAPI; if (api?.createAgent) return api.createAgent(data); throw new Error('IPC createAgent not available');
}
export async function updateAgentIPC(id: string, data: any) {
  const api = (window as any).chatAPI; if (api?.updateAgent) return api.updateAgent(id, data); throw new Error('IPC updateAgent not available');
}
export async function deleteAgentIPC(id: string) {
  const api = (window as any).chatAPI; if (api?.deleteAgent) return api.deleteAgent(id); throw new Error('IPC deleteAgent not available');
}

// ----- Applications -----
export async function getApplicationsIPC(userId?: string) {
  const api = (window as any).chatAPI; if (api?.getApplications) return api.getApplications(userId); throw new Error('IPC getApplications not available');
}
export async function getApplicationIPC(id: string) {
  const api = (window as any).chatAPI; if (api?.getApplication) return api.getApplication(id); throw new Error('IPC getApplication not available');
}
export async function createApplicationIPC(data: any) {
  const api = (window as any).chatAPI; if (api?.createApplication) return api.createApplication(data); throw new Error('IPC createApplication not available');
}
export async function updateApplicationIPC(id: string, data: any) {
  const api = (window as any).chatAPI; if (api?.updateApplication) return api.updateApplication(id, data); throw new Error('IPC updateApplication not available');
}
export async function deleteApplicationIPC(id: string) {
  const api = (window as any).chatAPI; if (api?.deleteApplication) return api.deleteApplication(id); throw new Error('IPC deleteApplication not available');
}

export async function submitTaskReviewDecisionIPC(reviewId: string, payload: any) {
  const api = (window as any).chatAPI;
  if (api?.submitTaskReviewDecision) {
    return api.submitTaskReviewDecision(reviewId, payload);
  }
  throw new Error('IPC submitTaskReviewDecision not available');
}
