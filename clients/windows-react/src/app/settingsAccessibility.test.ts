import { describe, expect, it } from "vitest";
import { getSettingsFocusableElements } from "./settingsAccessibility";

describe("settings accessibility focus contract", () => {
  it("keeps enabled visible controls in DOM order and excludes disabled/hidden controls", () => {
    document.body.innerHTML = `
      <section>
        <button id="first">First</button>
        <button disabled id="disabled">Disabled</button>
        <button hidden id="hidden">Hidden</button>
        <input id="second" />
        <div style="display:none"><a href="#" id="collapsed">Collapsed</a></div>
        <button id="third">Third</button>
      </section>
    `;

    expect(getSettingsFocusableElements(document.querySelector("section")! ).map((element) => element.id))
      .toEqual(["first", "second", "third"]);
  });
});

