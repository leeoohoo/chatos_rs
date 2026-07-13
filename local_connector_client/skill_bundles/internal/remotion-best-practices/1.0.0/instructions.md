# Remotion Best Practices

Use this Skill when building or reviewing programmatic video projects with React and Remotion.

Work locally in the authorized workspace. Prefer deterministic compositions, explicit dimensions and frame rates, reusable typed components, and assets whose loading behavior is known before rendering. Keep animation timing based on frames and composition FPS rather than wall-clock timers. Validate the composition with a short local preview before starting a full render.

For rendering:

1. Confirm Node.js, the project dependencies, and any required media tools are available locally.
2. Inspect the existing project scripts before choosing commands.
3. Avoid embedding secrets in source files or render arguments.
4. Use bounded concurrency and a writable output path inside the authorized workspace.
5. Report missing fonts, codecs, browser dependencies, or media files as local dependency errors.
6. Do not claim a video was rendered until the output file exists and has been inspected.

This prompt-only Skill provides workflow guidance. Any command execution must still use Local Connector terminal and approval controls.
