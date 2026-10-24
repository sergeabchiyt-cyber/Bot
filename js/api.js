/* ============================================================
 * XAUUSD Terminal — read-only REST snapshots.
 *
 * Only GET requests exist in this dashboard. These routes send
 * `Cache-Control: no-store` server-side; the client also asks for no-store
 * and never sends a body, credential, or authorization header.
 * ============================================================ */
(function (App) {
  "use strict";

  const { fetchPublicJson } = App.utils;

  function fetchStrategy() {
    return fetchPublicJson(App.config.endpoints.strategy, 8000);
  }

  function fetchDemo() {
    return fetchPublicJson(App.config.endpoints.demo, 8000);
  }

  function fetchStatus() {
    return fetchPublicJson(App.config.endpoints.status, 8000);
  }

  /** Node 4 `GET /diagnostics` — a bare `ExecutionSnapshot`, no envelope. */
  function fetchExecutionDiagnostics() {
    return fetchPublicJson(App.config.endpoints.executionDiagnostics, 8000);
  }

  App.api = { fetchStrategy, fetchDemo, fetchStatus, fetchExecutionDiagnostics };
})((window.App = window.App || {}));
