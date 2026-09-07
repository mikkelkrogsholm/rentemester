import { expect, test } from "bun:test";
import { selectPageDevToolsTarget } from "../../scripts/release/cockpit-evidence-cdp-target";

test("ignores extension background targets before the isolated about:blank page", () => {
  expect(
    selectPageDevToolsTarget([
      {
        type: "background_page",
        url: "chrome-extension://extension-id/background.html",
        webSocketDebuggerUrl: "ws://127.0.0.1/devtools/page/extension",
      },
      {
        type: "page",
        url: "https://unrelated.example/",
        webSocketDebuggerUrl: "ws://127.0.0.1/devtools/page/unrelated",
      },
      {
        type: "page",
        url: "about:blank",
        webSocketDebuggerUrl: "ws://127.0.0.1/devtools/page/blank",
      },
    ]),
  ).toBe("ws://127.0.0.1/devtools/page/blank");
});

test("rejects extension-only DevTools target lists", () => {
  expect(() =>
    selectPageDevToolsTarget([
      {
        type: "background_page",
        url: "chrome-extension://extension-id/background.html",
        webSocketDebuggerUrl: "ws://127.0.0.1/devtools/page/extension",
      },
      {
        type: "service_worker",
        url: "chrome-extension://extension-id/worker.js",
        webSocketDebuggerUrl: "ws://127.0.0.1/devtools/page/worker",
      },
    ]),
  ).toThrow("Chrome did not expose a DevTools page target");
});
