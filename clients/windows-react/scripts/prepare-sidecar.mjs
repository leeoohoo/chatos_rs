import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(projectRoot, "../..");
const hostManifest = join(repositoryRoot, "clients/local_agent_host/Cargo.toml");
const isDebug = process.env.TAURI_ENV_DEBUG === "true";
const profile = isDebug ? "debug" : "release";

const cargoArguments = [
  "build",
  "--manifest-path",
  hostManifest,
  "--bin",
  "chatos_local_agent_host",
];
if (!isDebug) cargoArguments.push("--release");

const build = spawnSync("cargo", cargoArguments, {
  cwd: repositoryRoot,
  stdio: "inherit",
});
if (build.error) throw build.error;
if (build.status !== 0) process.exit(build.status ?? 1);

const metadata = spawnSync(
  "cargo",
  ["metadata", "--manifest-path", hostManifest, "--no-deps", "--format-version", "1"],
  { cwd: repositoryRoot, encoding: "utf8" },
);
if (metadata.error) throw metadata.error;
if (metadata.status !== 0) {
  process.stderr.write(metadata.stderr);
  process.exit(metadata.status ?? 1);
}

const targetDirectory = JSON.parse(metadata.stdout).target_directory;
const executableName = process.platform === "win32"
  ? "chatos_local_agent_host.exe"
  : "chatos_local_agent_host";
const source = join(targetDirectory, profile, executableName);
const destination = join(projectRoot, "src-tauri/resources", executableName);
mkdirSync(dirname(destination), { recursive: true });
copyFileSync(source, destination);
if (process.platform !== "win32") chmodSync(destination, 0o755);

process.stdout.write(`Prepared Local Agent Host resource: ${destination}\n`);
