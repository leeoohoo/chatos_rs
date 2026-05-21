import { describe, expect, it } from 'vitest';

import type { ProjectRunEnvironment } from '../../../types';
import {
  buildProjectRunEnvironmentUpdatePayload,
  resolveCustomToolchainEnvironment,
  resolveEnvVarsEnvironment,
  resolveSelectedToolchainEnvironment,
} from './projectRunnerEnvironmentActions';

const baseEnvironment: ProjectRunEnvironment = {
  projectId: 'project_1',
  optionsByKind: {},
  configFiles: [],
  validationIssues: [],
  selectedToolchains: {
    python: 'python:/usr/bin/python3',
  },
  customToolchains: {},
  envVars: {
    APP_ENV: 'dev',
  },
};

describe('projectRunnerEnvironmentActions', () => {
  it('applies a direct selected toolchain change optimistically', () => {
    expect(resolveSelectedToolchainEnvironment({
      environment: baseEnvironment,
      kind: 'python',
      optionId: 'python:/venv/bin/python',
    })).toEqual({
      ...baseEnvironment,
      selectedToolchains: {
        python: 'python:/venv/bin/python',
      },
    });
  });

  it('builds custom toolchain optimistic state and payloads from the latest draft path', () => {
    const resolved = resolveCustomToolchainEnvironment({
      environment: baseEnvironment,
      kind: 'python',
      draftPath: '/venv/bin/python',
    });

    expect(resolved.nextSelectedToolchains).toEqual({
      python: 'python:/venv/bin/python',
    });
    expect(resolved.nextCustomToolchains).toEqual({
      python: {
        kind: 'python',
        label: '手动指定: bin/python',
        path: '/venv/bin/python',
      },
    });
    expect(resolved.nextEnvironment).toEqual({
      ...baseEnvironment,
      selectedToolchains: {
        python: 'python:/venv/bin/python',
      },
      customToolchains: {
        python: {
          kind: 'python',
          label: '手动指定: bin/python',
          path: '/venv/bin/python',
        },
      },
    });
  });

  it('parses env var drafts once and applies the optimistic environment snapshot', () => {
    expect(resolveEnvVarsEnvironment({
      environment: baseEnvironment,
      envVarsDraft: 'APP_ENV=prod\nPORT=8080\n# note',
    })).toEqual({
      nextEnvironment: {
        ...baseEnvironment,
        envVars: {
          APP_ENV: 'prod',
          PORT: '8080',
        },
      },
      nextEnvVars: {
        APP_ENV: 'prod',
        PORT: '8080',
      },
    });
  });

  it('builds the direct environment update payload without extra adapter state', () => {
    expect(buildProjectRunEnvironmentUpdatePayload({
      selectedToolchains: { python: 'python:/venv/bin/python' },
      customToolchains: {
        python: {
          kind: 'python',
          label: 'manual python',
          path: '/venv/bin/python',
        },
      },
      envVars: { APP_ENV: 'dev' },
    })).toEqual({
      selected_toolchains: { python: 'python:/venv/bin/python' },
      custom_toolchains: {
        python: {
          kind: 'python',
          label: 'manual python',
          path: '/venv/bin/python',
        },
      },
      env_vars: { APP_ENV: 'dev' },
    });
  });
});
