// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { ProjectRunToolchainOption } from '../../../types';
import { buildProjectRunResolutionSuggestions } from './projectRunnerResolutionSuggestions';

const buildOption = (kind: string, path: string): ProjectRunToolchainOption => ({
  id: `${kind}:${path}`,
  kind,
  label: path,
  path,
  source: 'manual',
});

describe('projectRunnerResolutionSuggestions', () => {
  it('generates a toolchain suggestion for JDK mismatches', () => {
    expect(buildProjectRunResolutionSuggestions({
      diagnosis: 'JDK 版本与项目编译目标不匹配',
      selectedTarget: {
        id: 'target_1',
        label: 'Java App',
        kind: 'java',
        cwd: '/workspace',
        command: 'mvn spring-boot:run',
        source: 'analyzer',
        confidence: 1,
        requiredToolchains: ['java_home'],
      },
      runTargets: [],
      selectedToolchainOptions: {
        java_home: buildOption('java_home', '/usr/lib/jvm/java-17'),
      },
      availableOptionsByKind: {
        java_home: [
          buildOption('java_home', '/usr/lib/jvm/java-17'),
          buildOption('java_home', '/usr/lib/jvm/java-21'),
        ],
      },
    }).map((item) => item.id)).toContain('switch-java-home');
  });
});
