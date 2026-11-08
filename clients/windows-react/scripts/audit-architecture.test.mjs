import { describe, expect, it } from "vitest";
import { auditIconOnlyButtonAccessibleNames } from "./audit-architecture-rules.mjs";

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
