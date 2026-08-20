import { createWriteStream } from "node:fs";
import { chmod, copyFile, mkdir, rm } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const exec = promisify(execFile);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dest = path.join(root, "src-tauri", "resources");
const SCRCPY = "3.3.1";
const PLATFORM_TOOLS = {
  darwin: "https://dl.google.com/android/repository/platform-tools-latest-darwin.zip",
  win32: "https://dl.google.com/android/repository/platform-tools-latest-windows.zip",
  linux: "https://dl.google.com/android/repository/platform-tools-latest-linux.zip",
};

async function download(url, file) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${url} -> ${res.status}`);
  await pipeline(Readable.fromWeb(res.body), createWriteStream(file));
}

async function unzip(zipPath, outDir) {
  await mkdir(outDir, { recursive: true });
  if (process.platform === "win32") {
    await exec("powershell", [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `Expand-Archive -LiteralPath '${zipPath.replace(/'/g, "''")}' -DestinationPath '${outDir.replace(/'/g, "''")}' -Force`,
    ]);
    return;
  }
  const pyArgs = [
    "-c",
    "import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])",
    zipPath,
    outDir,
  ];
  try {
    await exec("python3", pyArgs);
  } catch {
    await exec("python", pyArgs);
  }
}

async function extractPlatformTools(osKey, files) {
  const url = PLATFORM_TOOLS[osKey];
  if (!url) return;
  const tmp = path.join(os.tmpdir(), `platform-tools-${osKey}.zip`);
  const unpacked = path.join(os.tmpdir(), `platform-tools-${osKey}`);
  console.log(`Downloading platform-tools (${osKey})…`);
  await download(url, tmp);
  await rm(unpacked, { recursive: true, force: true });
  await unzip(tmp, unpacked);
  const pt = path.join(unpacked, "platform-tools");
  for (const name of files) {
    await copyFile(path.join(pt, name), path.join(dest, name));
    if (!name.endsWith(".dll") && !name.endsWith(".exe")) {
      await chmod(path.join(dest, name), 0o755);
    }
    console.log(`Copied ${name}`);
  }
  await rm(tmp, { force: true });
  await rm(unpacked, { recursive: true, force: true });
}

await mkdir(dest, { recursive: true });

const jar = path.join(dest, "scrcpy-server");
console.log(`Downloading scrcpy-server ${SCRCPY}…`);
await download(
  `https://github.com/Genymobile/scrcpy/releases/download/v${SCRCPY}/scrcpy-server-v${SCRCPY}`,
  jar
);

const host = process.platform;
if (host === "darwin") {
  await extractPlatformTools("darwin", ["adb"]);
} else if (host === "linux") {
  await extractPlatformTools("linux", ["adb"]);
} else if (host === "win32") {
  await extractPlatformTools("win32", ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll"]);
}

if (host !== "win32") {
  // Stage Windows ADB so a Mac/Linux tree can still produce a Windows installer.
  await extractPlatformTools("win32", ["adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll"]);
}

console.log("Sidecars ready in src-tauri/resources/");
