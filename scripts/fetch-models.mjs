#!/usr/bin/env node

import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { mkdir, readFile, rename, rm, stat } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";

const platform = (process.argv[2] ?? "").toLowerCase();
if (!new Set(["windows", "android"]).has(platform)) {
  console.error("Usage: node scripts/fetch-models.mjs <windows|android>");
  process.exit(2);
}

const repoRoot = resolve(import.meta.dirname, "..");
const manifestPath = join(repoRoot, "models", "manifest.json");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const outputDir = join(repoRoot, "models", "runtime", platform);
await mkdir(outputDir, { recursive: true });

const selected = [];
for (const model of manifest.models) {
  const exact = model.variants.find(
    (variant) => variant.platform.toLowerCase() === platform,
  );
  const shared = model.variants.find((variant) => variant.platform === "SHARED");
  const variant = exact ?? shared;
  if (variant) selected.push({ model, variant });
}

for (const { model, variant } of selected) {
  await preparePinnedFile({
    label: `${model.id}@${model.version}`,
    file: variant.file,
    sourceUrl: variant.source_url,
    sha256: variant.sha256,
    sizeBytes: variant.size_bytes,
  });
}

if (platform === "windows") {
  await preparePinnedFile({
    label: "google-litert@2.2.0",
    file: "libLiteRt.dll",
    sourceUrl:
      "https://storage.googleapis.com/litert/binaries/2.2.0/windows_x86_64/libLiteRt.dll",
    sha256: "3f2b6ed9ccca4f8d8a298a9b49006fcf9b176ef27ea2c757d1ce552b736a5675",
    sizeBytes: 11908608,
  });
}

await readFile(manifestPath, "utf8");
console.log(`Prepared ${selected.length} pinned local models in ${outputDir}`);

async function preparePinnedFile({
  label,
  file,
  sourceUrl,
  sha256,
  sizeBytes,
}) {
  const target = join(outputDir, file);
  const expectedHash = sha256.toLowerCase();
  const current = await sha256IfExists(target);
  if (current === expectedHash) {
    if (sizeBytes) {
      const info = await stat(target);
      if (info.size !== sizeBytes) {
        await rm(target, { force: true });
      } else {
        console.log(`reuse ${label}: ${basename(target)}`);
        return;
      }
    } else {
      console.log(`reuse ${label}: ${basename(target)}`);
      return;
    }
  }

  const partial = `${target}.part`;
  await rm(partial, { force: true });
  console.log(`fetch ${label}: ${sourceUrl}`);
  await download(sourceUrl, partial);

  const downloadedHash = await sha256IfExists(partial);
  if (downloadedHash !== expectedHash) {
    await rm(partial, { force: true });
    throw new Error(
      `SHA256 mismatch for ${file}: expected ${sha256}, got ${downloadedHash}`,
    );
  }

  if (sizeBytes) {
    const info = await stat(partial);
    if (info.size !== sizeBytes) {
      await rm(partial, { force: true });
      throw new Error(
        `Size mismatch for ${file}: expected ${sizeBytes}, got ${info.size}`,
      );
    }
  }

  await rename(partial, target);
}

async function sha256IfExists(path) {
  try {
    const data = await readFile(path);
    return createHash("sha256").update(data).digest("hex");
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}

async function download(url, destination) {
  await mkdir(dirname(destination), { recursive: true });
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok || !response.body) {
    throw new Error(`Download failed (${response.status}) for ${url}`);
  }
  const file = createWriteStream(destination, { flags: "wx" });
  await pipeline(Readable.fromWeb(response.body), file);
}
