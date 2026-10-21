import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const macRuntime = resolve(root, "../macos/Sources/ChatOSCore/AgentSkills/Runtime");
const output = resolve(root, "src-tauri/resources/agent-skill-examples.json");

const professions = parseProfiles(resolve(macRuntime, "LocalAgentProgressiveSkillExamples+Professions.swift"));
const projectTypes = parseProfiles(resolve(macRuntime, "LocalAgentProgressiveSkillExamples+ProjectTypes.swift"));
if (Object.keys(professions).length !== 33 || Object.keys(projectTypes).length !== 27) {
  throw new Error(`Unexpected Skill example counts: ${Object.keys(professions).length}/${Object.keys(projectTypes).length}`);
}

mkdirSync(dirname(output), { recursive: true });
const generated = `${JSON.stringify({ professions, projectTypes }, null, 2)}\n`;
if (process.argv.includes("--check")) {
  if (readFileSync(output, "utf8") !== generated) {
    throw new Error("Agent Skill example snapshot is stale; run npm run generate:agent-skill-examples.");
  }
} else {
  writeFileSync(output, generated);
}

function parseProfiles(path) {
  const source = readFileSync(path, "utf8");
  const entry = /"([A-Za-z0-9_]+)":\s*\.init\(\s*zh:\s*\((.*?)\),\s*en:\s*\((.*?)\)\s*\)/gs;
  const result = {};
  for (const match of source.matchAll(entry)) {
    const zh = strings(match[2]);
    const en = strings(match[3]);
    if (zh.length !== 5 || en.length !== 5) throw new Error(`Invalid example tuple for ${match[1]}`);
    result[match[1]] = {
      situationZh: zh[0], goodExampleZh: zh[1], evidenceZh: zh[2],
      counterexampleZh: zh[3], correctionZh: zh[4],
      situationEn: en[0], goodExampleEn: en[1], evidenceEn: en[2],
      counterexampleEn: en[3], correctionEn: en[4],
    };
  }
  return result;
}

function strings(tuple) {
  return [...tuple.matchAll(/"([^"\\]*(?:\\.[^"\\]*)*)"/gs)].map((match) => JSON.parse(`"${match[1]}"`));
}
