import { describe, expect, it } from 'vitest';

import type {
  ProjectRunEnvironment,
  ProjectRunTarget,
  ProjectRunToolchainOption,
} from '../../../types';
import {
  buildCustomToolchainDrafts,
  buildCustomToolchainSelectionState,
  buildEnvPreview,
  buildEnvironmentHints,
  buildMissingToolchainKinds,
  buildSelectedToolchainOptions,
  parseEnvVarsDraft,
  resolveCommandPreview,
  serializeEnvVarsDraft,
} from './projectRunnerEnvironmentState';

const buildOption = (
  kind: string,
  path: string,
  overrides: Partial<ProjectRunToolchainOption> = {},
): ProjectRunToolchainOption => ({
  id: `${kind}:${path}`,
  kind,
  label: path.split('/').slice(-1)[0] || path,
  path,
  source: 'manual',
  ...overrides,
});

describe('projectRunnerEnvironmentState', () => {
  it('resolves command preview with selected binaries and maven settings', () => {
    expect(resolveCommandPreview('python app.py', {
      python: buildOption('python', '/venv/bin/python'),
    })).toBe('/venv/bin/python app.py');

    expect(resolveCommandPreview('mvn test', {
      mvn: buildOption('mvn', '/opt/maven/bin/mvn'),
      mvn_settings: buildOption('mvn_settings', '/tmp/settings.xml'),
    })).toBe('/opt/maven/bin/mvn test');

    expect(resolveCommandPreview('./mvnw test', {
      mvn_settings: buildOption('mvn_settings', '/tmp/settings.xml'),
    })).toBe('./mvnw -s /tmp/settings.xml test');
  });

  it('serializes and parses env var drafts deterministically', () => {
    const draft = serializeEnvVarsDraft({
      PORT: '3000',
      APP_ENV: 'dev',
    });
    expect(draft).toBe('APP_ENV=dev\nPORT=3000');
    expect(parseEnvVarsDraft(' APP_ENV=dev \n# note\nPORT = 3000\nbroken')).toEqual({
      APP_ENV: 'dev',
      PORT: '3000',
    });
  });

  it('builds selected toolchain state, drafts and missing kinds from environment', () => {
    const environment: ProjectRunEnvironment = {
      projectId: 'project_1',
      optionsByKind: {
        python: [buildOption('python', '/venv/bin/python')],
        node: [],
      },
      configFiles: [],
      validationIssues: [],
      selectedToolchains: {
        python: 'python:/venv/bin/python',
      },
      customToolchains: {
        python: {
          kind: 'python',
          label: 'manual python',
          path: '/venv/bin/python',
        },
      },
      envVars: {},
    };

    expect(buildSelectedToolchainOptions(environment, ['python', 'node'])).toEqual({
      python: buildOption('python', '/venv/bin/python'),
      node: null,
    });
    expect(buildMissingToolchainKinds(['python', 'node'], environment)).toEqual(['node']);
    expect(buildCustomToolchainDrafts(environment, ['python', 'node'])).toEqual({
      python: '/venv/bin/python',
      node: '',
    });
  });

  it('builds env preview and user hints for selected target', () => {
    const target: ProjectRunTarget = {
      id: 'target_1',
      label: 'Run App',
      kind: 'java',
      cwd: '/workspace',
      command: 'mvn spring-boot:run',
      source: 'analyzer',
      confidence: 1,
      requiredToolchains: ['java_home', 'mvn', 'mvn_settings'],
    };

    const selectedOptions = {
      java_home: buildOption('java_home', '/jdk'),
      mvn: buildOption('mvn', '/maven/bin/mvn'),
      mvn_settings: buildOption('mvn_settings', '/tmp/settings.xml'),
    } satisfies Record<string, ProjectRunToolchainOption | null>;

    expect(buildEnvPreview({ APP_ENV: 'dev' }, selectedOptions)).toBe(
      'APP_ENV=dev\nJAVA_HOME=/jdk\nMVN_BIN=/maven/bin/mvn\nMVN_SETTINGS=/tmp/settings.xml',
    );
    expect(buildEnvironmentHints(target, selectedOptions)).toEqual([
      '启动前会自动注入 JAVA_HOME=/jdk',
      'Maven 命令会自动追加 -s /tmp/settings.xml',
      '系统 Maven 命令会替换为 /maven/bin/mvn',
    ]);
  });

  it('builds custom toolchain selection payloads', () => {
    expect(buildCustomToolchainSelectionState(
      'python',
      '/opt/venv/bin/python',
      {},
      {},
    )).toEqual({
      selectedOptionId: 'python:/opt/venv/bin/python',
      customToolchains: {
        python: {
          kind: 'python',
          label: '手动指定: bin/python',
          path: '/opt/venv/bin/python',
        },
      },
      selectedToolchains: {
        python: 'python:/opt/venv/bin/python',
      },
    });
  });
});
