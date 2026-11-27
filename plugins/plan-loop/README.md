# Plan Loop

Plan Loop is a persistent ChatOS workflow plugin for macOS. It requires a
requirement-aware planning task before execution work, binds execution tasks to
the confirmed plan, and encourages autonomous recovery from technical blockers.

## Included components

- `plan-loop`: plan-first orchestration instructions.
- `plan-loop-planner`: planning-task research and plan publication.
- `plan-loop-recovery`: blocker recovery and focused replanning.
- Native ChatOS session and prompt hooks that keep the workflow active while
  the plugin is enabled. A separate Codex Hook manifest is included only for
  Codex-compatible installation.

Version `0.1.4` supports macOS. Windows support is intentionally deferred.
