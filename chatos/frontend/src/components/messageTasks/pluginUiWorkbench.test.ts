// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';
import { pluginArtifactBridgeDescriptor } from './PluginUiWorkbenchCard';
import {
  isPluginUiBridgeMessageSource,
  parsePluginUiBridgeIncoming,
  pluginUiBridgeResponse,
  pluginUiReadySummariesFromEvents,
  validatePluginArtifactListResponse,
  validatePluginArtifactReadResponse,
  validatePluginArtifactWriteResponse,
  validatePluginUiWorkbenchSession,
} from './pluginUiWorkbench';

const readyEvent = (): MessageTaskRunnerRunEvent => ({
  id: 'event-1',
  run_id: 'run-1',
  event_type: 'plugin_ui_ready',
  payload: {
    event_schema_version: 1,
    run_id: 'run-1',
    device_id: 'device-1',
    plugin_id: 'plugin-1',
    release_id: 'release-1',
    artifact_sha256: 'a'.repeat(64),
    component_key: 'workbench',
    adapter_session_id: 'adapter-1',
    ui: {
      title: 'Workbench',
      surface: 'workbench',
      bridge_protocol_version: 1,
      bridge_capabilities: [
        'host.context.read',
        'artifact.list',
        'artifact.read',
        'artifact.download',
        'artifact.create',
        'artifact.update',
      ],
      artifact_mime_types: ['application/pdf', 'application/json'],
      snapshot_sha256: 'b'.repeat(64),
      html: '<script>should not project</script>',
    },
  },
});

describe('Plugin UI Workbench security projection', () => {
  it('projects only the immutable ready descriptor needed by the host', () => {
    const ready = pluginUiReadySummariesFromEvents([readyEvent()]);
    expect(ready).toEqual([{
      eventId: 'event-1',
      runId: 'run-1',
      pluginId: 'plugin-1',
      releaseId: 'release-1',
      artifactSha256: 'a'.repeat(64),
      componentKey: 'workbench',
      adapterSessionId: 'adapter-1',
      snapshotSha256: 'b'.repeat(64),
      title: 'Workbench',
      surface: 'workbench',
      bridgeProtocolVersion: 1,
      bridgeCapabilities: [
        'host.context.read',
        'artifact.list',
        'artifact.read',
        'artifact.download',
        'artifact.create',
        'artifact.update',
      ],
      artifactMimeTypes: ['application/pdf', 'application/json'],
    }]);
    expect(JSON.stringify(ready)).not.toContain('should not project');
  });

  it('rejects session identity drift and non-workbench iframe paths', () => {
    const ready = pluginUiReadySummariesFromEvents([readyEvent()])[0];
    const sessionId = `pui_${'c'.repeat(64)}`;
    const session = {
      session_id: sessionId,
      expires_in: 300,
      expires_at: '2026-07-26T00:05:00Z',
      iframe_path: `/api/plugin-ui/workbench/${sessionId}/ui/index.html#chatos_plugin_ui_v1&protocol_version=1&adapter_session_id=adapter-1&host_session_nonce=puih_${'d'.repeat(64)}`,
      bridge_protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: `puih_${'d'.repeat(64)}`,
      bridge_capabilities: [
        'host.context.read',
        'artifact.list',
        'artifact.read',
        'artifact.download',
      ],
      host_context: {
        run_id: 'run-1',
        plugin_id: 'plugin-1',
        release_id: 'release-1',
        component_key: 'workbench',
        title: 'Workbench',
        surface: 'workbench',
      },
    };
    expect(validatePluginUiWorkbenchSession(session, ready)).toEqual(session);
    const isolated = {
      ...session,
      iframe_path: `https://plugin-ui.example.com${session.iframe_path}`,
    };
    expect(validatePluginUiWorkbenchSession(isolated, ready)).toEqual(isolated);
    expect(validatePluginUiWorkbenchSession({
      ...isolated,
      iframe_path: isolated.iframe_path.replace('https://', 'http://'),
    }, ready)).toBeNull();
    expect(validatePluginUiWorkbenchSession({
      ...isolated,
      iframe_path: isolated.iframe_path.replace(
        'https://plugin-ui.example.com',
        'https://user@plugin-ui.example.com',
      ),
    }, ready)).toBeNull();
    const projected = validatePluginUiWorkbenchSession({
      ...session,
      raw_secret: 'should-not-project',
      host_context: {
        ...session.host_context,
        prompt: 'private prompt',
      },
    }, ready);
    expect(JSON.stringify(projected)).not.toContain('should-not-project');
    expect(JSON.stringify(projected)).not.toContain('private prompt');
    expect(validatePluginUiWorkbenchSession({
      ...session,
      iframe_path: 'https://attacker.example/plugin-ui',
    }, ready)).toBeNull();
    expect(validatePluginUiWorkbenchSession({
      ...session,
      adapter_session_id: 'adapter-2',
    }, ready)).toBeNull();
  });

  it('accepts only exact opaque bridge ready/request schemas', () => {
    expect(parsePluginUiBridgeIncoming({
      type: 'chatos.plugin_ui.ready',
      protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: 'nonce-1',
    })).toEqual({
      kind: 'ready',
      adapterSessionId: 'adapter-1',
      hostSessionNonce: 'nonce-1',
    });
    const request = parsePluginUiBridgeIncoming({
      type: 'chatos.plugin_ui.request',
      protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: 'nonce-1',
      request_id: 'request-1',
      method: 'host.context.read',
      payload: {},
    });
    expect(request).toEqual({
      kind: 'request',
      adapterSessionId: 'adapter-1',
      hostSessionNonce: 'nonce-1',
      requestId: 'request-1',
      method: 'host.context.read',
      payload: {},
    });
    expect(parsePluginUiBridgeIncoming({
      type: 'chatos.plugin_ui.request',
      protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: 'nonce-1',
      request_id: 'request-1',
      method: 'host.context.read',
      payload: {},
      unexpected: true,
    })).toBeNull();
    expect(parsePluginUiBridgeIncoming({
      type: 'chatos.plugin_ui.request',
      protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: 'nonce-1',
      request_id: 'request-2',
      method: 'artifact.read',
      payload: { artifact_id: `pa_${'c'.repeat(32)}` },
    })).toEqual({
      kind: 'request',
      adapterSessionId: 'adapter-1',
      hostSessionNonce: 'nonce-1',
      requestId: 'request-2',
      method: 'artifact.read',
      payload: { artifact_id: `pa_${'c'.repeat(32)}` },
    });
    expect(parsePluginUiBridgeIncoming({
      type: 'chatos.plugin_ui.request',
      protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: 'nonce-1',
      request_id: 'request-3',
      method: 'artifact.read',
      payload: {},
    })).toBeNull();
    expect(parsePluginUiBridgeIncoming({
      type: 'chatos.plugin_ui.request',
      protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: 'nonce-1',
      request_id: 'request-create',
      method: 'artifact.create',
      payload: {
        display_name: 'draft.json',
        media_type: 'application/json',
        body_base64: 'e30=',
      },
    })).toMatchObject({
      kind: 'request',
      method: 'artifact.create',
      payload: {
        display_name: 'draft.json',
        media_type: 'application/json',
        body_base64: 'e30=',
      },
    });
    expect(parsePluginUiBridgeIncoming({
      type: 'chatos.plugin_ui.request',
      protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: 'nonce-1',
      request_id: 'request-update',
      method: 'artifact.update',
      payload: {
        artifact_id: `pa_${'c'.repeat(32)}`,
        expected_sha256: 'd'.repeat(64),
        body_base64: 'e30=',
      },
    })).toMatchObject({
      kind: 'request',
      method: 'artifact.update',
      payload: {
        artifact_id: `pa_${'c'.repeat(32)}`,
        expected_sha256: 'd'.repeat(64),
        body_base64: 'e30=',
      },
    });
    expect(parsePluginUiBridgeIncoming({
      type: 'chatos.plugin_ui.request',
      protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: 'nonce-1',
      request_id: 'request-bad-base64',
      method: 'artifact.create',
      payload: {
        display_name: 'draft.json',
        media_type: 'application/json',
        body_base64: 'e30',
      },
    })).toBeNull();
  });

  it('validates Artifact list/read responses against the UI Run identity', () => {
    const ready = pluginUiReadySummariesFromEvents([readyEvent()])[0];
    const access = {
      run_id: ready.runId,
      plugin_id: ready.pluginId,
      release_id: ready.releaseId,
      artifact_sha256: 'a'.repeat(64),
      component_key: ready.componentKey,
      adapter_session_id: ready.adapterSessionId,
      ui_snapshot_sha256: ready.snapshotSha256,
    };
    const artifact = {
      artifact_id: `pa_${'c'.repeat(32)}`,
      owner: {
        owner_user_id: 'user-1',
        run_id: ready.runId,
        device_id: 'device-1',
        workspace_id: 'workspace-1',
        plugin_id: ready.pluginId,
        release_id: ready.releaseId,
        artifact_sha256: 'a'.repeat(64),
        component_key: 'documents',
        adapter_session_id: 'producer-1',
      },
      workspace_relative_path: 'artifacts/report.pdf',
      display_name: 'report.pdf',
      media_type: 'application/pdf',
      size_bytes: 4,
      sha256: 'd'.repeat(64),
      created_at: '2026-07-26T00:00:00Z',
      producer_tool_name: 'create_text_pdf',
      downloadable: true,
      mutable: false,
    };
    expect(validatePluginArtifactListResponse({ access, artifacts: [artifact] }, ready))
      .not.toBeNull();
    expect(validatePluginArtifactReadResponse({
      access,
      artifact,
      body_base64: 'JVBERg==',
    }, ready, artifact.artifact_id)).not.toBeNull();
    expect(validatePluginArtifactListResponse({
      access,
      artifacts: [{ ...artifact, media_type: 'text/html' }],
    }, ready)).toBeNull();
    const projected = pluginArtifactBridgeDescriptor(artifact);
    expect(projected).not.toHaveProperty('owner');
    expect(projected).not.toHaveProperty('workspace_relative_path');
    expect(JSON.stringify(projected)).not.toContain('workspace-1');
    expect(JSON.stringify(projected)).not.toContain('user-1');
  });

  it('accepts only exact UI-owned mutable write responses and projects no local identity', () => {
    const ready = pluginUiReadySummariesFromEvents([readyEvent()])[0];
    const access = {
      run_id: ready.runId,
      plugin_id: ready.pluginId,
      release_id: ready.releaseId,
      artifact_sha256: ready.artifactSha256,
      component_key: ready.componentKey,
      adapter_session_id: ready.adapterSessionId,
      ui_snapshot_sha256: ready.snapshotSha256,
    };
    const artifact = {
      artifact_id: `pa_${'e'.repeat(32)}`,
      owner: {
        owner_user_id: 'user-1',
        run_id: ready.runId,
        device_id: 'device-1',
        workspace_id: 'workspace-1',
        plugin_id: ready.pluginId,
        release_id: ready.releaseId,
        artifact_sha256: ready.artifactSha256,
        component_key: ready.componentKey,
        adapter_session_id: ready.adapterSessionId,
      },
      workspace_relative_path: `chatos-plugin-artifacts/opaque/pa_${'e'.repeat(32)}/draft.json`,
      display_name: 'draft.json',
      media_type: 'application/json',
      size_bytes: 2,
      sha256: 'f'.repeat(64),
      created_at: '2026-07-26T00:00:00Z',
      producer_tool_name: 'artifact.create',
      downloadable: true,
      mutable: true,
    };
    const response = validatePluginArtifactWriteResponse({
      access,
      operation: 'create',
      artifact,
    }, ready, 'create', null, {
      display_name: 'draft.json',
      media_type: 'application/json',
    }, 'e30=');
    expect(response).not.toBeNull();
    const projected = {
      operation: response!.operation,
      artifact: pluginArtifactBridgeDescriptor(response!.artifact),
    };
    expect(projected).not.toHaveProperty('access');
    expect(projected.artifact).not.toHaveProperty('owner');
    expect(projected.artifact).not.toHaveProperty('workspace_relative_path');
    expect(JSON.stringify(projected)).not.toContain('workspace-1');

    expect(validatePluginArtifactWriteResponse({
      access,
      operation: 'create',
      artifact: {
        ...artifact,
        owner: { ...artifact.owner, adapter_session_id: 'other-session' },
      },
    }, ready, 'create', null, {
      display_name: 'draft.json',
      media_type: 'application/json',
    }, 'e30=')).toBeNull();
  });

  it('requires the exact iframe WindowProxy and opaque origin', () => {
    const expected = {} as WindowProxy;
    const other = {} as WindowProxy;
    expect(isPluginUiBridgeMessageSource('null', expected, expected)).toBe(true);
    expect(isPluginUiBridgeMessageSource('https://app.example.com', expected, expected)).toBe(false);
    expect(isPluginUiBridgeMessageSource('null', other, expected)).toBe(false);
  });

  it('builds responses without widening the negotiated session identity', () => {
    const ready = pluginUiReadySummariesFromEvents([readyEvent()])[0];
    const sessionId = `pui_${'c'.repeat(64)}`;
    const session = validatePluginUiWorkbenchSession({
      session_id: sessionId,
      expires_in: 300,
      expires_at: '2026-07-26T00:05:00Z',
      iframe_path: `/api/plugin-ui/workbench/${sessionId}/ui/index.html#chatos_plugin_ui_v1&protocol_version=1&adapter_session_id=adapter-1&host_session_nonce=puih_${'d'.repeat(64)}`,
      bridge_protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: `puih_${'d'.repeat(64)}`,
      bridge_capabilities: ['host.context.read'],
      host_context: {
        run_id: 'run-1',
        plugin_id: 'plugin-1',
        release_id: 'release-1',
        component_key: 'workbench',
        title: 'Workbench',
        surface: 'workbench',
      },
    }, ready);
    expect(session).not.toBeNull();
    expect(pluginUiBridgeResponse(session!, 'request-1', true, session!.host_context, null)).toMatchObject({
      type: 'chatos.plugin_ui.response',
      protocol_version: 1,
      adapter_session_id: 'adapter-1',
      host_session_nonce: `puih_${'d'.repeat(64)}`,
      request_id: 'request-1',
      ok: true,
      error_code: null,
    });
  });
});
