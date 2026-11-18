import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";
import { auditIconOnlyButtonAccessibleNames } from "./audit-architecture-rules.mjs";

const agentManagementStylesheet = readFileSync(
  resolve(process.cwd(), "src/features/agent-management/ui/agentManagement.css"),
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
