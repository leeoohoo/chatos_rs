// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import {
  mergeSessionRuntimeIntoMetadata,
  readSessionRuntimeFromMetadata,
} from './sessionRuntime';

describe('sessionRuntime metadata helpers', () => {
  it('reads runtime settings from source_metadata when session metadata is wrapped by the engine', () => {
    const metadata = {
      legacy_session_mapping: {
        session_id: 'session_1',
        project_id: 'project_1',
        agent_id: 'agent_1',
      },
      source_metadata: {
        chat_runtime: {
          selected_model_id: 'model_1',
          contact_agent_id: 'agent_1',
          contact_id: 'contact_1',
          workspace_root: '/tmp/workspace',
          auto_create_task: true,
          enabled_mcp_ids: ['mcp_a'],
        },
        ui_chat_selection: {
          selected_model_id: 'model_1',
          selected_agent_id: 'agent_1',
        },
        contact: {
          agent_id: 'agent_1',
          contact_id: 'contact_1',
        },
      },
    };

    expect(readSessionRuntimeFromMetadata(metadata)).toEqual({
      contactAgentId: 'agent_1',
      contactId: 'contact_1',
      remoteConnectionId: null,
      selectedModelId: 'model_1',
      selectedModelName: null,
      selectedThinkingLevel: null,
      projectId: null,
      projectRoot: null,
      workspaceRoot: '/tmp/workspace',
    });
  });

  it('writes runtime changes back into source_metadata when metadata is wrapped by the engine', () => {
    const metadata = {
      legacy_session_mapping: {
        session_id: 'session_1',
      },
      source_metadata: {
        chat_runtime: {
          selected_model_id: 'model_old',
          auto_create_task: false,
        },
      },
    };

    const merged = mergeSessionRuntimeIntoMetadata(metadata, {
      selectedModelId: 'model_new',
      workspaceRoot: '/tmp/next-workspace',
    });

    expect(merged).toMatchObject({
      legacy_session_mapping: {
        session_id: 'session_1',
      },
      source_metadata: {
        chat_runtime: {
          selected_model_id: 'model_new',
          workspace_root: '/tmp/next-workspace',
        },
        ui_chat_selection: {
          selected_model_id: 'model_new',
        },
      },
    });
  });

  it('does not mutate a read-only source_metadata object when merging runtime settings', () => {
    const sourceMetadata = Object.freeze({
      chat_runtime: Object.freeze({
        selected_model_id: 'model_old',
        auto_create_task: false,
      }),
    });
    const metadata = Object.freeze({
      legacy_session_mapping: {
        session_id: 'session_1',
      },
      source_metadata: sourceMetadata,
    });

    const merged = mergeSessionRuntimeIntoMetadata(metadata, {
      selectedModelId: 'model_new',
      workspaceRoot: '/tmp/next-workspace',
    });

    expect(merged).toMatchObject({
      legacy_session_mapping: {
        session_id: 'session_1',
      },
      source_metadata: {
        chat_runtime: {
          selected_model_id: 'model_new',
          workspace_root: '/tmp/next-workspace',
        },
      },
    });
    expect(merged.source_metadata).not.toBe(sourceMetadata);
    expect(sourceMetadata.chat_runtime).toEqual({
      selected_model_id: 'model_old',
      auto_create_task: false,
    });
  });
});
