"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");

global.window = { App: {} };
require("../js/normalize.js");
require("../js/utils.js");
const fetchPublicJson = global.window.App.utils.fetchPublicJson;
const originalFetch = global.fetch;

test("public snapshot helper uses bounded credential-free GET requests", async () => {
  let request;
  global.fetch = async (url, options) => {
    request = { url, options };
    return { ok: true, json: async () => ({ value: 0 }) };
  };
  try {
    assert.deepEqual(await fetchPublicJson("https://node.example/diagnostics"), { value: 0 });
    assert.equal(request.options.method, "GET");
    assert.equal(request.options.cache, "no-store");
    assert.deepEqual(request.options.headers, { Accept: "application/json" });
    assert.equal(Object.keys(request.options.headers).some((key) => key.toLowerCase() === "authorization"), false);
    assert.equal(request.url, "https://node.example/diagnostics");
  } finally {
    global.fetch = originalFetch;
  }
});

test("public snapshot helper reports HTTP errors and aborts stalled requests", async () => {
  global.fetch = async () => ({ ok: false, status: 503, json: async () => ({}) });
  try {
    await assert.rejects(fetchPublicJson("https://node.example/diagnostics"), /503/);

    global.fetch = (_url, options) => new Promise((_resolve, reject) => {
      options.signal.addEventListener("abort", () => {
        const error = new Error("aborted");
        error.name = "AbortError";
        reject(error);
      }, { once: true });
    });
    await assert.rejects(fetchPublicJson("https://node.example/diagnostics", 1000), /timed out after 1s/);
  } finally {
    global.fetch = originalFetch;
  }
});
