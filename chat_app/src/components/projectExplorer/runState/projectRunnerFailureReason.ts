import type { TranslateFn } from '../../../i18n/I18nProvider';
import type { TerminalLogResponse } from '../../../lib/api/client/types';

const normalizeLine = (value: string): string => value.trim();

const findLastMatchingLine = (lines: string[], patterns: RegExp[]): string | null => {
  for (let i = lines.length - 1; i >= 0; i -= 1) {
    const line = lines[i];
    if (patterns.some((pattern) => pattern.test(line))) {
      return line;
    }
  }
  return null;
};

export const extractFailureReasonFromLogs = (
  logs: TerminalLogResponse[],
  command: string,
  t?: TranslateFn,
): string | null => {
  const translate = t || ((key: string) => key);
  const lines = logs
    .map((item) => String(item?.content || ''))
    .join('\n')
    .split(/\r?\n/)
    .map(normalizeLine)
    .filter(Boolean);
  if (!lines.length) {
    return null;
  }

  const specializedChecks: Array<{ patterns: RegExp[]; reasonKey?: string }> = [
    {
      patterns: [
        /could not find or load main class/i,
        /main method not found in class/i,
        /no main manifest attribute/i,
      ],
    },
    {
      patterns: [
        /release version .* not supported/i,
        /invalid target release/i,
        /source option .* is no longer supported/i,
        /target option .* is no longer supported/i,
        /unsupported class file major version/i,
      ],
      reasonKey: 'runSettings.failure.jdkMismatch',
    },
    {
      patterns: [
        /java_home.*not defined correctly/i,
        /the java_home environment variable is not defined correctly/i,
      ],
      reasonKey: 'runSettings.failure.invalidJavaHome',
    },
    {
      patterns: [
        /non-parseable settings/i,
        /settings\.xml/i,
      ],
      reasonKey: 'runSettings.failure.invalidMavenSettings',
    },
    {
      patterns: [
        /could not resolve dependencies/i,
        /could not transfer artifact/i,
        /transfer failed for/i,
        /received status code 40[137]/i,
        /not authorized/i,
        /authentication failed/i,
        /proxy authentication/i,
      ],
      reasonKey: 'runSettings.failure.mavenDependencies',
    },
    {
      patterns: [
        /gradle.*requires java/i,
        /unsupported class file major version/i,
        /this version of gradle/i,
      ],
      reasonKey: 'runSettings.failure.gradleJdkMismatch',
    },
    {
      patterns: [
        /gradlew: permission denied/i,
        /permission denied.*gradlew/i,
        /wrapper.*permission denied/i,
      ],
      reasonKey: 'runSettings.failure.gradleWrapperPermission',
    },
    {
      patterns: [
        /no bin target named/i,
        /a bin target must be available/i,
        /no targets specified in the manifest/i,
      ],
      reasonKey: 'runSettings.failure.rustEntrypoint',
    },
    {
      patterns: [
        /could not compile/i,
        /error(\[e\d+\])?:/i,
      ],
      reasonKey: 'runSettings.failure.rustCompile',
    },
    {
      patterns: [
        /no go files/i,
        /package .* is not in std/i,
        /go: cannot find main module/i,
        /go\.mod file not found/i,
      ],
      reasonKey: 'runSettings.failure.goEntrypoint',
    },
    {
      patterns: [
        /listen tcp .* bind: address already in use/i,
        /address already in use/i,
        /eaddrinuse/i,
      ],
      reasonKey: 'runSettings.failure.portInUse',
    },
    {
      patterns: [
        /modulenotfounderror/i,
        /no module named/i,
        /can\'t open file/i,
        /pytest: command not found/i,
      ],
      reasonKey: 'runSettings.failure.pythonRuntime',
    },
    {
      patterns: [
        /missing script:/i,
        /enoent/i,
        /cannot find module/i,
      ],
    },
    {
      patterns: [
        /command not found/i,
        /no such file or directory/i,
      ],
    },
    {
      patterns: [
        /permission denied/i,
      ],
    },
    {
      patterns: [
        /traceback \(most recent call last\)/i,
        /\bpanic\b/i,
        /\bexception\b/i,
        /\berror\b/i,
        /\bfailed\b/i,
      ],
    },
  ];

  for (const check of specializedChecks) {
    const matched = findLastMatchingLine(lines, check.patterns);
    if (!matched) {
      continue;
    }
    return check.reasonKey ? translate(check.reasonKey) : matched;
  }

  const cmd = command.toLowerCase();
  const likelyLongRunning = /(run|start|dev|serve|bootrun|spring-boot:run)/i.test(cmd)
    && !/(test|build|lint)/i.test(cmd);
  if (likelyLongRunning) {
    return translate('runSettings.failure.longRunningExited');
  }
  return null;
};
