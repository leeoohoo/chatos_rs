import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const skillsRoot = path.join(projectRoot, "skills");

function markdownFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const location = path.join(directory, entry.name);
    return entry.isDirectory()
      ? markdownFiles(location)
      : entry.name.endsWith(".md")
        ? [location]
        : [];
  });
}

function readSkill(relativePath) {
  return readFileSync(path.join(skillsRoot, relativePath), "utf8");
}

test("shared Computer Use Skills do not describe the plugin as macOS-only", () => {
  const forbiddenClaims = [
    /visible local macOS interaction/i,
    /foreground macOS application/i,
    /verified macOS mouse/i,
    /visible macOS targets/i,
    /physical macOS arrow/i,
  ];

  for (const file of markdownFiles(skillsRoot)) {
    const contents = readFileSync(file, "utf8");
    for (const claim of forbiddenClaims) {
      assert.doesNotMatch(contents, claim, path.relative(projectRoot, file));
    }
  }
});

test("router documents both capability and activation contracts", () => {
  const router = readSkill("visual-computer-use/SKILL.md");

  assert.match(router, /Screen Recording or Accessibility/);
  assert.match(router, /interactive-desktop, capture, input, or Administrator\/UIPI/);
  assert.match(router, /macOS requires `bundle_identifier`/);
  assert.match(router, /Windows requires `application`/);
  assert.match(router, /real screenshots|screenshot.*source of truth/i);
  assert.match(router, /Move and visually verify.*before every click/i);
  assert.match(router, /Never claim success from an input event alone/i);
});

test("leaf Skills preserve platform-specific focus, coordinates, and shortcuts", () => {
  const focus = readSkill("visual-application-focus/SKILL.md");
  const targeting = readSkill("visual-observation-targeting/SKILL.md");
  const interaction = readSkill("visual-pointer-keyboard/SKILL.md");

  assert.match(focus, /macOS, pass `bundle_identifier`/);
  assert.match(focus, /Windows, pass `application`/);
  assert.match(focus, /process name, executable, Shell application name, or path/);
  assert.match(targeting, /macOS coordinates are global display points/);
  assert.match(targeting, /Windows coordinates are virtual-desktop physical pixels/);
  assert.match(targeting, /may be negative/);
  assert.match(targeting, /current screenshot metadata/);
  assert.match(interaction, /macOS commonly uses `Command`/);
  assert.match(interaction, /Windows commonly uses `Control`/);
});
