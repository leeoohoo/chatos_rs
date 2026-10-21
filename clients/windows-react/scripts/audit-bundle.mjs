import { readFile, stat } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const indexPath = path.join(root, "dist", "index.html");
const indexHtml = await readFile(indexPath, "utf8").catch(() => {
  throw new Error("dist/index.html 不存在；请先运行 npm run build。");
});
const entrySource = indexHtml.match(/<script[^>]+type=["']module["'][^>]+src=["']([^"']+)["']/i)?.[1]
  ?? indexHtml.match(/<script[^>]+src=["']([^"']+)["'][^>]+type=["']module["']/i)?.[1];

if (!entrySource || !entrySource.startsWith("/assets/") || entrySource.includes("..")) {
  throw new Error("无法从 dist/index.html 解析可信的 Vite 入口脚本。");
}

const entryPath = path.join(root, "dist", entrySource.slice(1));
const entryBytes = (await stat(entryPath)).size;
const entryContent = await readFile(entryPath);
const entryGzipBytes = gzipSync(entryContent).byteLength;

// The authenticated shell, Host gate and global approval overlay intentionally stay eager.
// Optional workspaces must remain behind dynamic imports. These ceilings leave roughly 12%
// headroom over the 2026-10-21 baseline (410.35 KiB raw / 126.14 KiB gzip).
const limits = {
  raw: 460 * 1024,
  gzip: 142 * 1024,
};

const formatKiB = (bytes) => `${(bytes / 1024).toFixed(2)} KiB`;
const failures = [];
if (entryBytes > limits.raw) {
  failures.push(`raw ${formatKiB(entryBytes)} > ${formatKiB(limits.raw)}`);
}
if (entryGzipBytes > limits.gzip) {
  failures.push(`gzip ${formatKiB(entryGzipBytes)} > ${formatKiB(limits.gzip)}`);
}

if (failures.length > 0) {
  throw new Error(
    `Windows React 初始入口包超出预算：${failures.join(", ")}。` +
    "请将低频页面、Adapter 或大型依赖恢复为动态导入。",
  );
}

console.log(
  `Windows React bundle budget passed: ${path.basename(entryPath)} ` +
  `(${formatKiB(entryBytes)} raw / ${formatKiB(entryGzipBytes)} gzip).`,
);
