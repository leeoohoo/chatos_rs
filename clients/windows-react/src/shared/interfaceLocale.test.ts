import { describe, expect, it } from "vitest";
import { interfaceText } from "./interfaceLocale";

describe("interfaceText", () => {
  it("projects the same shell key into the selected interface language", () => {
    expect(interfaceText("zh-CN", "workbench")).toBe("工作台");
    expect(interfaceText("en-US", "workbench")).toBe("Workbench");
    expect(interfaceText("en-US", "localServiceUnavailable")).toBe("Local service is not ready");
  });
});
