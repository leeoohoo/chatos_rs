#!/usr/bin/env node

import { randomUUID } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { crc32 } from "node:zlib";

const TEMPLATE_PREFIX = "artifact-template-";
const WRITE_LOCK_NAME = ".artifact-template-write-lock";
const LOCK_OWNER_FILENAME = "owner-pid";
const LOCK_OWNER_GRACE_MS = 30_000;
const MAX_SKILL_NAME_LENGTH = 64;
const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const artifactKinds = new Map([
  [
    ".docx",
    {
      kind: "document",
      pluginMention: "@documents",
      pluginUri: "plugin://documents@openai-primary-runtime",
      preservation:
        "Preserve page setup, sections, styles, lists, tables, headers, footers, and recurring page elements.",
    },
  ],
  [
    ".pptx",
    {
      kind: "presentation",
      pluginMention: "@presentations",
      pluginUri: "plugin://presentations@openai-primary-runtime",
      preservation:
        "Preserve source slides, layouts, masters, typography, geometry, images, charts, tables, and recurring slide chrome.",
    },
  ],
  [
    ".xlsx",
    {
      kind: "spreadsheet",
      pluginMention: "@spreadsheets",
      pluginUri: "plugin://spreadsheets@openai-primary-runtime",
      preservation:
        "Preserve sheet structure, formulas, names, number formats, dimensions, tables, charts, validation, conditional formatting, and frozen panes.",
    },
  ],
]);

async function createTemplateSkill(
  rawRequest,
  codexHome = getDefaultCodexHome(),
) {
  const request = await validateRequest(rawRequest);
  const artifact = artifactKinds.get(
    path.extname(request.referencePath).toLowerCase(),
  );
  const skillsRoot = path.join(codexHome, "skills");
  const releaseLock = await acquireWriteLock(
    path.join(codexHome, WRITE_LOCK_NAME),
  );
  try {
    const identity =
      request.mode === "update"
        ? await getUpdateIdentity(skillsRoot, request, artifact.kind)
        : await getCreateIdentity(skillsRoot, request.title);
    const skillPath = path.join(skillsRoot, identity.skillName);
    const stagedSkill = await stageTemplateSkill({
      artifact,
      description: request.description,
      displayName: identity.displayName,
      parentDirectory: skillsRoot,
      previewPath: request.previewPath,
      referencePath: request.referencePath,
      skillName: identity.skillName,
      sourceSkillPath: request.mode === "update" ? skillPath : null,
    });
    try {
      if (request.mode === "update") {
        await replaceTemplateSkill(stagedSkill, skillPath);
      } else {
        await fs.rename(stagedSkill, skillPath);
      }
    } catch (error) {
      await fs.rm(stagedSkill, { force: true, recursive: true });
      throw error;
    }
    return {
      displayName: identity.displayName,
      kind: artifact.kind,
      skillName: identity.skillName,
      skillPath,
    };
  } finally {
    await releaseLock();
  }
}

async function validateRequest(rawRequest) {
  if (
    rawRequest == null ||
    typeof rawRequest !== "object" ||
    Array.isArray(rawRequest)
  ) {
    throw new Error("The request must be a JSON object.");
  }
  if ("destination" in rawRequest) {
    throw new Error(
      "Template Creator only manages personal skills; destination is not supported.",
    );
  }
  const mode = rawRequest.mode ?? "create";
  if (mode !== "create" && mode !== "update") {
    throw new Error("mode must be 'create' or 'update'.");
  }
  const title = getRequiredString(rawRequest, "title");
  const description = getRequiredString(rawRequest, "description");
  assertSingleLine(title, "title", 64);
  assertSingleLine(description, "description", 600);
  const referencePath = path.resolve(
    getRequiredString(rawRequest, "referencePath"),
  );
  const previewPath = path.resolve(
    getRequiredString(rawRequest, "previewPath"),
  );
  const extension = path.extname(referencePath).toLowerCase();
  if (!artifactKinds.has(extension)) {
    throw new Error("referencePath must end in .docx, .pptx, or .xlsx.");
  }
  await Promise.all([
    assertRegularFile(referencePath, "referencePath"),
    assertRegularFile(previewPath, "previewPath"),
  ]);
  if (path.extname(previewPath).toLowerCase() !== ".png") {
    throw new Error("previewPath must end in .png.");
  }
  if (!hasValidPngStructure(await fs.readFile(previewPath))) {
    throw new Error("previewPath must contain a valid PNG.");
  }

  const templateName = getOptionalString(rawRequest, "templateName");
  if (mode === "update") {
    if (templateName == null) {
      throw new Error("templateName is required for an explicit update.");
    }
    assertTemplateName(templateName);
  } else if (templateName != null) {
    throw new Error("templateName is only valid when mode is 'update'.");
  }

  return {
    description,
    mode,
    previewPath,
    referencePath,
    templateName,
    title,
  };
}

function getDefaultCodexHome() {
  const homeDir = path.resolve(
    process.env.HOME ?? process.env.USERPROFILE ?? os.homedir(),
  );
  return path.resolve(process.env.CODEX_HOME ?? path.join(homeDir, ".codex"));
}

async function getCreateIdentity(skillsRoot, title) {
  const slug = getSlug(title);
  for (let index = 1; ; index += 1) {
    const suffix = index === 1 ? "" : `-${index}`;
    const baseLength = MAX_SKILL_NAME_LENGTH - suffix.length;
    const skillName =
      `${TEMPLATE_PREFIX}${slug}`.slice(0, baseLength).replace(/-+$/u, "") +
      suffix;
    if (!(await pathExists(path.join(skillsRoot, skillName)))) {
      return {
        displayName: index === 1 ? title : `${title} ${index}`,
        skillName,
      };
    }
  }
}

async function getUpdateIdentity(skillsRoot, request, kind) {
  const skillPath = path.join(skillsRoot, request.templateName);
  const sidecarPath = path.join(skillPath, "artifact-template.json");
  const sidecar = JSON.parse(await fs.readFile(sidecarPath, "utf8"));
  if (sidecar.schemaVersion !== 1 || sidecar.kind !== kind) {
    throw new Error(
      `${request.templateName} is not a version 1 ${kind} artifact template.`,
    );
  }
  return { displayName: request.title, skillName: request.templateName };
}

async function stageTemplateSkill({
  artifact,
  description,
  displayName,
  parentDirectory,
  previewPath,
  referencePath,
  skillName,
  sourceSkillPath,
}) {
  await fs.mkdir(parentDirectory, { recursive: true });
  const stagedSkill = await fs.mkdtemp(
    path.join(parentDirectory, `.${skillName}-stage-`),
  );
  const referenceFilename = `reference${path.extname(referencePath).toLowerCase()}`;
  try {
    if (sourceSkillPath != null) {
      await fs.cp(sourceSkillPath, stagedSkill, { recursive: true });
    }
    await Promise.all([
      fs.mkdir(path.join(stagedSkill, "agents"), { recursive: true }),
      fs.mkdir(path.join(stagedSkill, "assets"), { recursive: true }),
    ]);
    await Promise.all([
      fs.writeFile(
        path.join(stagedSkill, "SKILL.md"),
        getTemplateSkillMarkdown({
          artifact,
          description,
          displayName,
          skillName,
        }),
      ),
      fs.writeFile(
        path.join(stagedSkill, "agents", "openai.yaml"),
        getTemplateOpenAiYaml({ artifact, displayName, skillName }),
      ),
      fs.writeFile(
        path.join(stagedSkill, "artifact-template.json"),
        `${JSON.stringify(
          {
            schemaVersion: 1,
            kind: artifact.kind,
            reference: `assets/${referenceFilename}`,
            preview: "assets/preview.png",
          },
          null,
          2,
        )}\n`,
      ),
      fs.copyFile(
        referencePath,
        path.join(stagedSkill, "assets", referenceFilename),
      ),
      fs.copyFile(previewPath, path.join(stagedSkill, "assets", "preview.png")),
    ]);
    return stagedSkill;
  } catch (error) {
    await fs.rm(stagedSkill, { force: true, recursive: true });
    throw error;
  }
}

function getTemplateSkillMarkdown({
  artifact,
  description,
  displayName,
  skillName,
}) {
  const triggerDescription = `Create a ${artifact.kind} using the ${displayName} template and its retained reference file. Use when the user selects this template, names ${displayName}, or explicitly invokes $${skillName}. ${description}`;
  return `---
name: ${skillName}
description: ${JSON.stringify(triggerDescription)}
---

# ${displayName}

Create a new ${artifact.kind} from this template. Keep the reference file unchanged.

## Workflow

1. Read \`artifact-template.json\` and resolve its paths relative to this skill directory.
2. Load [${artifact.pluginMention}](${artifact.pluginUri}) and invoke its reference/template workflow with the retained file.
3. Treat the user's prompt and available sources as the content input. Do not invent facts merely to fill a template slot.
4. Clone or import the reference instead of replacing its visual system with generic defaults.
5. Render and verify the finished ${artifact.kind}, then return the final artifact.

## Fidelity

${artifact.preservation}

User instructions control requested content and explicit deviations. The retained reference controls layout and formatting where the user has not requested a change.
`;
}

function getTemplateOpenAiYaml({ artifact, displayName, skillName }) {
  const candidate = `Create ${artifact.kind}s with the ${displayName} template`;
  const shortDescription =
    candidate.length <= 64
      ? candidate
      : `Create a ${artifact.kind} from this saved template`;
  return `interface:
  display_name: ${JSON.stringify(displayName)}
  short_description: ${JSON.stringify(shortDescription)}
  icon_large: "./assets/preview.png"
  default_prompt: ${JSON.stringify(`Use $${skillName} to create a new ${artifact.kind} with this template.`)}
policy:
  allow_implicit_invocation: true
`;
}

async function replaceTemplateSkill(stagedPath, finalPath) {
  const backupPath = `${finalPath}.backup-${randomUUID()}`;
  await fs.rename(finalPath, backupPath);
  try {
    await fs.rename(stagedPath, finalPath);
  } catch (error) {
    try {
      await fs.rename(backupPath, finalPath);
    } catch (rollbackError) {
      throw new AggregateError(
        [error, rollbackError],
        "Template update failed and rollback was incomplete.",
      );
    }
    throw error;
  }
  await fs.rm(backupPath, { force: true, recursive: true });
}

async function acquireWriteLock(lockPath) {
  await fs.mkdir(path.dirname(lockPath), { recursive: true });
  let canRecover = true;
  while (true) {
    try {
      await fs.mkdir(lockPath);
      break;
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }
      if (canRecover && (await isWriteLockStale(lockPath))) {
        await fs.rm(lockPath, { force: true, recursive: true });
        canRecover = false;
        continue;
      }
      throw new Error(
        `Another artifact template write is already in progress at ${lockPath}.`,
      );
    }
  }
  try {
    await fs.writeFile(
      path.join(lockPath, LOCK_OWNER_FILENAME),
      `${process.pid}\n`,
    );
  } catch (error) {
    await fs.rm(lockPath, { force: true, recursive: true });
    throw error;
  }
  return () => fs.rm(lockPath, { force: true, recursive: true });
}

async function isWriteLockStale(lockPath) {
  const owner = await fs
    .readFile(path.join(lockPath, LOCK_OWNER_FILENAME), "utf8")
    .catch((error) => {
      if (error?.code === "ENOENT") {
        return null;
      }
      throw error;
    });
  const ownerPid = Number(owner);
  if (Number.isSafeInteger(ownerPid) && ownerPid > 0) {
    try {
      process.kill(ownerPid, 0);
      return false;
    } catch (error) {
      if (error?.code === "ESRCH") {
        return true;
      }
      if (error?.code === "EPERM") {
        return false;
      }
      throw error;
    }
  }
  return Date.now() - (await fs.stat(lockPath)).mtimeMs > LOCK_OWNER_GRACE_MS;
}

function getSlug(title) {
  const slug = title
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/gu, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/gu, "-")
    .replace(/^-+|-+$/gu, "");
  if (slug.length === 0) {
    throw new Error("title must contain at least one ASCII letter or number.");
  }
  return slug;
}

function assertTemplateName(templateName) {
  if (
    templateName.length > MAX_SKILL_NAME_LENGTH ||
    !/^artifact-template-[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(templateName)
  ) {
    throw new Error(
      "templateName must be a valid artifact-template skill name.",
    );
  }
}

function assertSingleLine(value, label, maxLength) {
  if (/[<>]/u.test(value)) {
    throw new Error(`${label} must not contain angle brackets.`);
  }
  if (value.length > maxLength || /[\0\r\n]/u.test(value)) {
    throw new Error(
      `${label} must be one line of at most ${maxLength} characters.`,
    );
  }
}

function hasValidPngStructure(bytes) {
  if (
    bytes.length < PNG_SIGNATURE.length ||
    !bytes.subarray(0, PNG_SIGNATURE.length).equals(PNG_SIGNATURE)
  ) {
    return false;
  }
  let offset = PNG_SIGNATURE.length;
  let firstChunk = true;
  while (offset + 12 <= bytes.length) {
    const dataLength = bytes.readUInt32BE(offset);
    const dataStart = offset + 8;
    const crcOffset = dataStart + dataLength;
    const nextOffset = crcOffset + 4;
    if (nextOffset > bytes.length) {
      return false;
    }
    const type = bytes.toString("ascii", offset + 4, dataStart);
    if (firstChunk && (type !== "IHDR" || dataLength !== 13)) {
      return false;
    }
    if (
      crc32(bytes.subarray(offset + 4, crcOffset)) !==
      bytes.readUInt32BE(crcOffset)
    ) {
      return false;
    }
    if (type === "IEND") {
      return dataLength === 0 && nextOffset === bytes.length;
    }
    firstChunk = false;
    offset = nextOffset;
  }
  return false;
}

async function assertRegularFile(filePath, label) {
  const stat = await fs.stat(filePath);
  if (!stat.isFile()) {
    throw new Error(`${label} must point to a file.`);
  }
}

function getRequiredString(value, key) {
  const entry = value[key];
  if (typeof entry !== "string" || entry.trim().length === 0) {
    throw new Error(`${key} must be a non-empty string.`);
  }
  return entry.trim();
}

function getOptionalString(value, key) {
  const entry = value[key];
  if (entry == null) {
    return null;
  }
  if (typeof entry !== "string" || entry.trim().length === 0) {
    throw new Error(`${key} must be a non-empty string when provided.`);
  }
  return entry.trim();
}

async function pathExists(filePath) {
  return fs
    .access(filePath)
    .then(() => true)
    .catch((error) => {
      if (error?.code === "ENOENT") {
        return false;
      }
      throw error;
    });
}

async function main() {
  if (process.argv.length !== 4 || process.argv[2] !== "--request") {
    throw new Error(
      "Usage: create-template-skill.mjs --request <request.json>",
    );
  }
  const request = JSON.parse(
    await fs.readFile(path.resolve(process.argv[3]), "utf8"),
  );
  const result = await createTemplateSkill(request);
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

if (
  process.argv[1] != null &&
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1])
) {
  main().catch((error) => {
    process.stderr.write(
      `${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 1;
  });
}
