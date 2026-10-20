import { readdirSync, readFileSync } from "node:fs";
import { extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));
const sourceRoot = join(projectRoot, "src");
const violations = [];

for (const file of sourceFiles(sourceRoot)) {
  const source = readFileSync(file, "utf8");
  const path = relative(projectRoot, file).split(sep).join("/");
  const isTest = /\.(test|spec)\.[^.]+$/.test(path);
  const isAdapter = path.includes("/adapters/") || path.includes("/platform/");
  const isCompositionRoot = path.startsWith("src/app/") || path === "src/main.tsx";
  const isFeatureUi = path.includes("/features/") && path.includes("/ui/");

  if (!isTest && source.includes("@tauri-apps/api") && !isAdapter) {
    violations.push(`${path}: Tauri APIs are only allowed in adapters/platform code`);
  }
  if (!isTest && /\binvoke\s*\(/.test(source) && !isAdapter) {
    violations.push(`${path}: direct invoke() calls are only allowed in typed adapters`);
  }
  if (isFeatureUi && /(?:from|import\s*)\s*["'][^"']*\/adapters\//.test(source)) {
    violations.push(`${path}: feature UI must depend on ports, not adapters`);
  }
  if (!isTest && !isCompositionRoot && /(?:from|import\s*)\s*["'][^"']*\/adapters\//.test(source) && !isAdapter) {
    violations.push(`${path}: adapters may only be wired by a composition root`);
  }
}

if (violations.length > 0) {
  process.stderr.write(`Windows React architecture violations:\n${violations.map((item) => `- ${item}`).join("\n")}\n`);
  process.exit(1);
}

process.stdout.write("Windows React architecture audit passed.\n");

function* sourceFiles(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      yield* sourceFiles(path);
    } else if ([".ts", ".tsx"].includes(extname(entry.name))) {
      yield path;
    }
  }
}
