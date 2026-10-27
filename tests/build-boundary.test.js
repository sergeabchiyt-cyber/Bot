"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const root = path.resolve(__dirname, "..");
const runtimeDir = path.join(root, "js");
const distDir = path.join(root, "dist");

function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const absolute = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(absolute) : [absolute];
  });
}

const forbidden = [
  /localhost/i,
  /127\.0\.0\.1/,
  /NODE4_SHARED_TOKEN/i,
  /NODE4_HOST/i,
  /STRATEGY_HOST/i,
  /node4_token/i,
  /shared[_-]?secret/i,
  /\/execution\b/i,
  /["'`]\/strategy\b/i,
  /["'`]\/demo\b/i,
  /method\s*:\s*["'](?:POST|PUT|PATCH|DELETE)["']/i,
];

test("runtime sources own transport only in client.js and have no private route", () => {
  const files = fs.readdirSync(runtimeDir).filter((file) => file.endsWith(".js"));
  assert.ok(files.includes("client.js"));
  for (const file of files) {
    const source = fs.readFileSync(path.join(runtimeDir, file), "utf8");
    for (const pattern of forbidden) {
      assert.equal(pattern.test(source), false, `${file} matches ${pattern}`);
    }
    if (file !== "client.js") {
      assert.doesNotMatch(source, /\bfetch\s*\(/, `${file} constructs a request`);
      assert.doesNotMatch(source, /new\s+WebSocket\s*\(/, `${file} constructs a socket`);
    }
  }
  const index = fs.readFileSync(path.join(root, "index.html"), "utf8");
  for (const pattern of forbidden) {
    assert.equal(pattern.test(index), false, `index.html matches ${pattern}`);
  }
});

test("npm run build emits a clean read-only static bundle", () => {
  execFileSync(process.execPath, [path.join(root, "scripts", "build.js")], {
    cwd: root,
    stdio: "pipe",
  });
  const files = walk(distDir);
  assert.ok(files.some((file) => file.endsWith(`${path.sep}index.html`)));
  assert.ok(files.some((file) => file.endsWith(`${path.sep}client.js`)));
  for (const absolute of files) {
    const relative = path.relative(distDir, absolute).replace(/\\/g, "/");
    const source = fs.readFileSync(absolute, "utf8");
    for (const pattern of forbidden) {
      assert.equal(pattern.test(source), false, `dist/${relative} matches ${pattern}`);
    }
    if (relative !== "js/client.js") {
      assert.doesNotMatch(source, /\bfetch\s*\(/, `dist/${relative} constructs a request`);
      assert.doesNotMatch(source, /new\s+WebSocket\s*\(/, `dist/${relative} constructs a socket`);
    }
  }
});
