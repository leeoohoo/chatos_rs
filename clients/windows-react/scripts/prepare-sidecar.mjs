import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(projectRoot, "../..");
const hostManifest = join(repositoryRoot, "clients/local_agent_host/Cargo.toml");

export function resolveBuildConfiguration(environment = process.env, platform = process.platform) {
  const targetTriple = environment.TAURI_ENV_TARGET_TRIPLE?.trim() || null;
  if (targetTriple && !/^[A-Za-z0-9_.-]+$/.test(targetTriple)) {
    throw new Error(`Invalid TAURI_ENV_TARGET_TRIPLE: ${targetTriple}`);
  }

  const isDebug = environment.TAURI_ENV_DEBUG === "true";
  const targetPlatform = environment.TAURI_ENV_PLATFORM?.trim().toLowerCase();
  const isWindows = targetPlatform
    ? targetPlatform === "windows"
    : targetTriple
      ? targetTriple.split("-").includes("windows")
      : platform === "win32";

  return {
    targetTriple,
    profile: isDebug ? "debug" : "release",
    executableName: isWindows
      ? "chatos_local_agent_host.exe"
      : "chatos_local_agent_host",
  };
}

export function buildCargoArguments(configuration) {
  const argumentsList = [
    "build",
    "--manifest-path",
    hostManifest,
    "--bin",
    "chatos_local_agent_host",
  ];
  if (configuration.targetTriple) {
    argumentsList.push("--target", configuration.targetTriple);
  }
  if (configuration.profile === "release") argumentsList.push("--release");
  return argumentsList;
}

export function resolveBuiltExecutable(targetDirectory, configuration) {
  return join(
    targetDirectory,
    ...(configuration.targetTriple ? [configuration.targetTriple] : []),
    configuration.profile,
    configuration.executableName,
  );
}

function run() {
  const configuration = resolveBuildConfiguration();
  const build = spawnSync("cargo", buildCargoArguments(configuration), {
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
  const source = resolveBuiltExecutable(targetDirectory, configuration);
  const destination = join(
    projectRoot,
    "src-tauri/resources",
    configuration.executableName,
  );
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  if (!configuration.executableName.endsWith(".exe")) chmodSync(destination, 0o755);

  process.stdout.write(
    `Prepared Local Agent Host resource (${configuration.targetTriple ?? "host"}): ${destination}\n`,
  );
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) run();
