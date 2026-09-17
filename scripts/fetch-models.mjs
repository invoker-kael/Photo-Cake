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
  const target = join(outputDir, variant.file);
  const current = await sha256IfExists(target);
  if (current === variant.sha256.toLowerCase()) {
    console.log(`reuse ${model.id}@${model.version}: ${basename(target)}`);
    continue;
  }

  const partial = `${target}.part`;
  await rm(partial, { force: true });
  console.log(`fetch ${model.id}@${model.version}: ${variant.source_url}`);
  await download(variant.source_url, partial);

  const downloadedHash = await sha256IfExists(partial);
  if (downloadedHash !== variant.sha256.toLowerCase()) {
    await rm(partial, { force: true });
    throw new Error(
      `SHA256 mismatch for ${variant.file}: expected ${variant.sha256}, got ${downloadedHash}`,
    );
  }

  if (variant.size_bytes) {
    const info = await stat(partial);
    if (info.size !== variant.size_bytes) {
      await rm(partial, { force: true });
      throw new Error(
        `Size mismatch for ${variant.file}: expected ${variant.size_bytes}, got ${info.size}`,
      );
    }
  }

  await rename(partial, target);
}

await readFile(manifestPath, "utf8");
console.log(`Prepared ${selected.length} pinned local models in ${outputDir}`);

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
