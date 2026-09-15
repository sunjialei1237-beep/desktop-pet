#!/usr/bin/env node
// 一条命令完成发布：NSIS 打包 → 签名（免交互）→ 生成 latest.json。
//
// 用法（先关闭正在运行的「璃」，否则链接 exe 时报 os error 5）：
//   node scripts/release.mjs --notes "本次更新说明"
//
// 产物（D:/cargo-target/desktop-pet/release/bundle/nsis/，跟随 .cargo/config.toml）：
//   Liri_<ver>_x64-setup.exe      上传到 GitHub Release
//   latest.json                   上传到 GitHub Release（tag = v<ver>）
//
// 注：签名走 `tauri signer sign -p ""` 命令行参数。不能只设
// TAURI_SIGNING_PRIVATE_KEY 环境变量——Windows 上空字符串的
// TAURI_SIGNING_PRIVATE_KEY_PASSWORD 等于未设置，CLI 会弹密码提示，
// 在脚本/CI 里永久挂起。
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync, existsSync, statSync } from "node:fs";
import { parseArgs } from "node:util";

const REPO = "sunjialei1237-beep/desktop-pet";
const KEY_PATH = ".tauri-signing/liri-updater.key";

const conf = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const version = conf.version;
// 根目录 .cargo/config.toml 可能把 target-dir 指到仓库外（D 盘大空间），尊重它。
let targetDir = "src-tauri/target";
const m = readFileSync(".cargo/config.toml", "utf8").match(/^target-dir\s*=\s*"(.+)"/m);
if (m) targetDir = m[1].replace(/\\/g, "/");
const nsisDir = `${targetDir}/release/bundle/nsis`;
const setupExe = `${nsisDir}/Liri_${version}_x64-setup.exe`;
const sigFile = `${setupExe}.sig`;

if (!existsSync(KEY_PATH)) {
  console.error(`找不到私钥 ${KEY_PATH} —— 丢失私钥将永远无法推送更新，请从备份恢复`);
  process.exit(1);
}

const started = Date.now();
console.log(`① 打包 v${version}（tauri build）…`);
const build = spawnSync("npm", ["run", "tauri", "build"], {
  stdio: "pipe",
  encoding: "utf8",
  shell: true,
});
const exeStat = existsSync(setupExe) ? statSync(setupExe) : null;
if (!exeStat || exeStat.mtimeMs < started - 5000) {
  // 排除签名步骤的预期报错后仍无新鲜产物 = 构建真失败了
  console.error((build.stderr || build.stdout || "").slice(-1500));
  console.error(
    "\n构建失败。若上面有 os error 5（拒绝访问）：请先关闭正在运行的「璃」再重试。"
  );
  process.exit(1);
}
console.log(`✓ 安装包 ${setupExe}`);

console.log("② 签名（更新包校验用）…");
const key = readFileSync(KEY_PATH, "utf8").trim();
// 不走 shell 直接调 CLI 的 JS 入口：shell 拼接会丢掉空字符串参数（-p ""），
// 导致后续路径被当成密码吞掉。
const sign = spawnSync(
  process.execPath,
  ["node_modules/@tauri-apps/cli/tauri.js", "signer", "sign", "-k", key, "-p", "", setupExe],
  { stdio: "pipe", encoding: "utf8", timeout: 120_000 }
);
if (sign.status !== 0 || !existsSync(sigFile)) {
  console.error((sign.stderr || sign.stdout || String(sign.error || "")).slice(-500));
  process.exit(1);
}
console.log(`✓ 签名 ${sigFile}`);

console.log("③ 生成 latest.json …");
const { values } = parseArgs({ options: { notes: { type: "string", default: "" } } });
const latest = {
  version,
  notes: values.notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(sigFile, "utf8").trim(),
      url: `https://github.com/${REPO}/releases/download/v${version}/Liri_${version}_x64-setup.exe`,
    },
  },
};
writeFileSync(`${nsisDir}/latest.json`, JSON.stringify(latest, null, 2));

console.log(`\n全部完成。发布：`);
console.log(`  1. GitHub 创建 Release，tag = v${version}`);
console.log(`  2. 上传 ${setupExe}`);
console.log(`  3. 上传 ${nsisDir}/latest.json`);
console.log(`  4. 发布后，用户在应用内「检查更新」即可一键覆盖安装`);
