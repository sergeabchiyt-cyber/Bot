#!/usr/bin/env node
/* ============================================================
 * Dependency-free static build.
 *
 * Copies only the assets index.html actually loads, then fails the build if
 * browser output contains a loopback URL, a private execution route, a retired
 * endpoint, a write verb, or a transport constructor outside js/client.js.
 * ============================================================ */
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const dist = path.join(root, "dist");
const indexPath = path.join(root, "index.html");
const index = fs.readFileSync(indexPath, "utf8");

fs.rmSync(dist, { recursive: true, force: true });
fs.mkdirSync(path.join(dist, "js"), { recursive: true });

const files = new Set(["index.html", "style.css"]);
for (const match of index.matchAll(/(?:src|href)="((?:js\/[\w.-]+\.js|style\.css))"/g)) {
  files.add(match[1]);
}
for (const file of files) {
  const from = path.join(root, file);
  const to = path.join(dist, file);
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.copyFileSync(from, to);
}

const banned = [
  { label: "loopback URL", pattern: /localhost|127\.0\.0\.1/i },
  { label: "private Node 4 config", pattern: /NODE4_HOST|STRATEGY_HOST/i },
  { label: "shared credential", pattern: /NODE4_SHARED_TOKEN|node4_token|shared[_-]?secret/i },
  { label: "private execution route", pattern: /\/execution\b/i },
  // Quote-anchored so the Node 3 hostname (which starts with "strategy-") is
  // not mistaken for the retired `/strategy` endpoint.
  { label: "retired Node 1 route", pattern: /["'`]\/strategy\b|["'`]\/demo\b/i },
  { label: "write request", pattern: /method\s*:\s*["'](?:POST|PUT|PATCH|DELETE)["']|fetch\([^)]*method\s*:\s*["'](?:POST|PUT|PATCH|DELETE)["']/i },
];

function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const absolute = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(absolute) : [absolute];
  });
}

const failures = [];
for (const absolute of walk(dist)) {
  const relative = path.relative(dist, absolute).replace(/\\/g, "/");
  const source = fs.readFileSync(absolute, "utf8");
  for (const rule of banned) {
    if (rule.pattern.test(source)) failures.push(`${relative}: ${rule.label}`);
  }
  if (relative !== "js/client.js") {
    if (/\bfetch\s*\(/.test(source)) failures.push(`${relative}: fetch() outside typed client`);
    if (/new\s+WebSocket\s*\(/.test(source)) failures.push(`${relative}: new WebSocket() outside typed client`);
  }
}

if (failures.length) {
  console.error("Build boundary check failed:\n - " + failures.join("\n - "));
  process.exitCode = 1;
} else {
  const manifest = {
    builtAt: new Date().toISOString(),
    files: [...files].sort(),
    boundary: "read-only Node 1 + Node 3 GET and /ws only",
  };
  fs.writeFileSync(path.join(dist, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Built ${files.size} static assets in dist/ (boundary check passed).`);
}
