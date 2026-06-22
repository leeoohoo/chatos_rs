import { describe, expect, it } from 'vitest';

import {
  mergeSessionAiSelectionIntoMetadata,
  readSessionAiSelectionFromMetadata,
} from './sessionAiSelection';

describe('sessionAiSelection helpers', () => {
  it('reads selected model and agent from source_metadata', () => {
    const metadata = {
      legacy_session_mapping: {
        session_id: 'session_1',
      },
      source_metadata: {
        chat_runtime: {
          selected_model_id: 'model_1',
          contact_agent_id: 'agent_1',
        },
        ui_chat_selection: {
          selected_model_id: 'model_1',
          selected_agent_id: 'agent_1',
        },
      },
    };

    expect(readSessionAiSelectionFromMetadata(metadata)).toEqual({
      selectedModelId: 'model_1',
      selectedAgentId: 'agent_1',
    });
  });

  it('writes selected model and agent into source_metadata when present', () => {
    const metadata = {
      legacy_session_mapping: {
        session_id: 'session_1',
      },
      source_metadata: {
        chat_runtime: {
          selected_model_id: 'model_old',
        },
      },
    };

    const merged = mergeSessionAiSelectionIntoMetadata(metadata, {
      selectedModelId: 'model_new',
      selectedAgentId: 'agent_2',
    });

    expect(merged).toMatchObject({
      legacy_session_mapping: {
        session_id: 'session_1',
      },
      source_metadata: {
        chat_runtime: {
          selected_model_id: 'model_new',
          contact_agent_id: 'agent_2',
        },
        ui_chat_selection: {
          selected_model_id: 'model_new',
          selected_agent_id: 'agent_2',
        },
        contact: {
          agent_id: 'agent_2',
        },
      },
    });
  });

  it('does not mutate a read-only source_metadata object when merging selection', () => {
    const sourceMetadata = Object.freeze({
      chat_runtime: Object.freeze({
        selected_model_id: 'model_old',
      }),
    });
    const metadata = Object.freeze({
      legacy_session_mapping: {
        session_id: 'session_1',
      },
      source_metadata: sourceMetadata,
    });

    const merged = mergeSessionAiSelectionIntoMetadata(metadata, {
      selectedModelId: 'model_new',
      selectedAgentId: 'agent_2',
    });

    expect(merged).toMatchObject({
      legacy_session_mapping: {
        session_id: 'session_1',
      },
      source_metadata: {
        chat_runtime: {
          selected_model_id: 'model_new',
          contact_agent_id: 'agent_2',
        },
        ui_chat_selection: {
          selected_model_id: 'model_new',
          selected_agent_id: 'agent_2',
        },
      },
    });
    expect(merged.source_metadata).not.toBe(sourceMetadata);
    expect(sourceMetadata.chat_runtime).toEqual({
      selected_model_id: 'model_old',
    });
  });
});
