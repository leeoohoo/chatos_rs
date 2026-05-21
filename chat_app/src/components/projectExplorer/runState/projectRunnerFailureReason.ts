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
): string | null => {
  const lines = logs
    .map((item) => String(item?.content || ''))
    .join('\n')
    .split(/\r?\n/)
    .map(normalizeLine)
    .filter(Boolean);
  if (!lines.length) {
    return null;
  }

  const specializedChecks: Array<{ patterns: RegExp[]; reason?: string }> = [
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
      reason: 'JDK 版本与项目编译目标不匹配',
    },
    {
      patterns: [
        /java_home.*not defined correctly/i,
        /the java_home environment variable is not defined correctly/i,
      ],
      reason: 'JAVA_HOME 配置无效',
    },
    {
      patterns: [
        /non-parseable settings/i,
        /settings\.xml/i,
      ],
      reason: 'Maven settings.xml 配置有误',
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
      reason: 'Maven 依赖下载失败，请检查仓库、认证或代理配置',
    },
    {
      patterns: [
        /gradle.*requires java/i,
        /unsupported class file major version/i,
        /this version of gradle/i,
      ],
      reason: 'Gradle 与当前 JDK 版本不匹配',
    },
    {
      patterns: [
        /gradlew: permission denied/i,
        /permission denied.*gradlew/i,
        /wrapper.*permission denied/i,
      ],
      reason: 'Gradle Wrapper 没有执行权限',
    },
    {
      patterns: [
        /no bin target named/i,
        /a bin target must be available/i,
        /no targets specified in the manifest/i,
      ],
      reason: 'Rust 可执行入口不存在或 bin 名称不匹配',
    },
    {
      patterns: [
        /could not compile/i,
        /error(\[e\d+\])?:/i,
      ],
      reason: 'Rust 编译失败，请检查代码或依赖配置',
    },
    {
      patterns: [
        /no go files/i,
        /package .* is not in std/i,
        /go: cannot find main module/i,
        /go\.mod file not found/i,
      ],
      reason: 'Go 入口或模块配置有误',
    },
    {
      patterns: [
        /listen tcp .* bind: address already in use/i,
        /address already in use/i,
        /eaddrinuse/i,
      ],
      reason: '端口已被占用',
    },
    {
      patterns: [
        /modulenotfounderror/i,
        /no module named/i,
        /can\'t open file/i,
        /pytest: command not found/i,
      ],
      reason: 'Python 解释器或依赖环境有误',
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
    return check.reason || matched;
  }

  const cmd = command.toLowerCase();
  const likelyLongRunning = /(run|start|dev|serve|bootrun|spring-boot:run)/i.test(cmd)
    && !/(test|build|lint)/i.test(cmd);
  if (likelyLongRunning) {
    return '命令已退出，未检测到持续运行进程';
  }
  return null;
};
