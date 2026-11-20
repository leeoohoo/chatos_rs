import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";
import {
  auditAccentBackgroundToken,
  auditAccentTextContrast,
  auditIconOnlyButtonAccessibleNames,
  auditSecondaryTextContrast,
} from "./audit-architecture-rules.mjs";

const agentManagementStylesheet = readFileSync(
  resolve(process.cwd(), "src/features/agent-management/ui/agentManagement.css"),
  "utf8",
);
const appShellSource = readFileSync(
  resolve(process.cwd(), "src/app/AppShell.tsx"),
  "utf8",
);
const themeStylesheet = readFileSync(
  resolve(process.cwd(), "src/design-system/theme.css"),
  "utf8",
);

describe("audit-architecture icon-only buttons", () => {
  it("reports an icon-only dismiss button without an accessible name", () => {
    const violations = [];

    auditIconOnlyButtonAccessibleNames(
      '<section>\n  <button onClick={close}>×</button>\n</section>',
      "src/example.tsx",
      violations,
    );

    expect(violations).toEqual([
      "src/example.tsx:2: icon-only buttons must provide aria-label or aria-labelledby",
    ]);
  });

  it("accepts aria-label and aria-labelledby names", () => {
    const violations = [];

    auditIconOnlyButtonAccessibleNames(
      '<button aria-label="Close">×</button>\n<button aria-labelledby="close-title">x</button>',
      "src/example.tsx",
      violations,
    );

    expect(violations).toEqual([]);
  });
});

describe("audit-architecture secondary text contrast", () => {
  it("rejects the previous low-contrast secondary label token", () => {
    const violations = [];

    auditSecondaryTextContrast(
      ":root { --secondary: rgba(60, 60, 67, 0.62); }",
      "src/design-system/theme.css",
      violations,
    );

    expect(violations).toEqual([
      "src/design-system/theme.css: --secondary text contrast must be at least 4.5:1 on core light surfaces (found 3.449:1)",
    ]);
  });

  it("accepts secondary text with contrast margin on every core surface", () => {
    const violations = [];

    auditSecondaryTextContrast(
      ":root { --secondary: rgba(60, 60, 67, 0.76); }",
      "src/design-system/theme.css",
      violations,
    );

    expect(violations).toEqual([]);
  });
});

describe("audit-architecture accent text contrast", () => {
  it("rejects the previous accent in light, action, and dark contexts", () => {
    const violations = [];

    auditAccentTextContrast(
      ":root { --accent: #0a84ff; --accent-action: #0a84ff; --accent-dark: #0a84ff; }\n"
        + "@media (prefers-color-scheme: dark) { :root { --accent: var(--accent-dark); } }",
      "src/design-system/theme.css",
      violations,
    );

    expect(violations).toEqual([
      "src/design-system/theme.css: --accent text contrast must be at least 4.5:1 on core light surfaces (found 3.240:1)",
      "src/design-system/theme.css: white action text contrast on --accent-action must be at least 4.5:1 (found 3.647:1)",
      "src/design-system/theme.css: --accent-dark text contrast must be at least 4.5:1 on core dark surfaces (found 3.919:1)",
    ]);
  });

  it("accepts separate foreground and action colors across themes", () => {
    const violations = [];

    auditAccentTextContrast(
      ":root { --accent: #0066cc; --accent-action: #0066cc; --accent-dark: #64b5ff; }\n"
        + "@media (prefers-color-scheme: dark) { :root { --accent: var(--accent-dark); } }",
      "src/design-system/theme.css",
      violations,
    );

    expect(violations).toEqual([]);
  });
});

describe("audit-architecture accent background token", () => {
  it("rejects a background that reuses the theme-dependent foreground", () => {
    const violations = [];

    auditAccentBackgroundToken(
      ".button { color: white; background: var(--accent); }",
      "src/example.css",
      violations,
    );

    expect(violations).toEqual([
      "src/example.css:1: backgrounds must use --accent-action so dark foreground contrast stays independent",
    ]);
  });

  it("accepts the action background token", () => {
    const violations = [];

    auditAccentBackgroundToken(
      ".button { color: white; background: var(--accent-action); }",
      "src/example.css",
      violations,
    );

    expect(violations).toEqual([]);
  });
});

describe("agent management responsive layout", () => {
  it("stacks the team dashboard before high-DPI windows overflow horizontally", () => {
    const compactRule = agentManagementStylesheet.match(/@media \(max-width: 1280px\) \{([\s\S]*?)\n\}/)?.[1] ?? "";

    expect(compactRule).toContain(".agent-team-dashboard-summary { grid-template-columns: minmax(0,1fr)");
    expect(compactRule).toContain(".agent-team-dashboard-metrics { flex-wrap: wrap; }");
    expect(compactRule).toContain(".agent-team-overview__body { grid-template-columns: minmax(0,1fr); }");
  });

  it("keeps Run event grid styles away from nested Markdown articles", () => {
    expect(agentManagementStylesheet).toContain(".agent-team-run-detail__event { display: grid;");
    expect(agentManagementStylesheet).not.toContain(".agent-team-run-detail__body > section article {");
  });

  it("keeps document Markdown typography inside team message bounds", () => {
    expect(agentManagementStylesheet).toContain(".agent-team-message .notepad-markdown { min-width: 0; overflow: visible; padding: 0;");
    expect(agentManagementStylesheet).toContain(".agent-team-message--system.is-rich { max-width: min(86%,820px);");
    expect(agentManagementStylesheet).toContain(".agent-team-message .notepad-markdown h1,.agent-team-message .notepad-markdown h2,.agent-team-message .notepad-markdown h3,.agent-team-message .notepad-markdown h4 { margin: .7em 0 .35em; font-size: inherit;");
    expect(agentManagementStylesheet).toContain(".agent-team-message .notepad-markdown p,.agent-team-message .notepad-markdown li,.agent-team-message .notepad-markdown blockquote { font-size: inherit; line-height: inherit; }");
  });
});

describe("settings overlay workspace preservation", () => {
  it("keeps the selected workspace mounted and non-interactive behind settings", () => {
    const workspaceSurface = 'className="content-surface content-surface--resource-workspace"';
    const settingsSurface = 'className="content-surface content-surface--settings"';

    expect(appShellSource).toContain("aria-hidden={settingsOpen}");
    expect(appShellSource).toContain(workspaceSurface);
    expect(appShellSource).toContain("{settingsOpen && (");
    expect(appShellSource).toContain(settingsSurface);
    expect(appShellSource.indexOf(workspaceSurface)).toBeLessThan(appShellSource.indexOf(settingsSurface));
    expect(themeStylesheet).toContain(
      '.content-surface--resource-workspace[aria-hidden="true"] { pointer-events: none; }',
    );
  });
});
