export type Cdp = {
  call(
    method: string,
    params?: Record<string, unknown>,
  ): Promise<Record<string, unknown>>;
};

/**
 * A protocol command is an ordered boundary in the DevTools session. Once its
 * response arrives, all earlier messages have passed through the connection's
 * synchronous event handler. This is a drain, not a timing delay.
 */
async function fenceBrowserEvents(cdp: Cdp): Promise<void> {
  await cdp.call("Runtime.evaluate", {
    expression: "undefined",
    returnByValue: true,
    awaitPromise: true,
  });
}

function throwForConsoleErrors(scenario: string, consoleErrors: string[]): void {
  if (consoleErrors.length)
    throw new Error(
      `console errors in ${scenario}: ${consoleErrors.join("\n")}`,
    );
}

/**
 * Capture only after the event stream is clean, then fence and check again.
 * Chrome may emit Log.entryAdded while screenshot capture is finalizing.
 */
export async function captureFinalizedScreenshot(options: {
  cdp: Cdp;
  scenario: string;
  consoleErrors: string[];
}): Promise<string> {
  await fenceBrowserEvents(options.cdp);
  throwForConsoleErrors(options.scenario, options.consoleErrors);
  const screenshot = await options.cdp.call("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  if (typeof screenshot.data !== "string")
    throw new Error("Chrome did not return PNG");
  await fenceBrowserEvents(options.cdp);
  throwForConsoleErrors(options.scenario, options.consoleErrors);
  return screenshot.data;
}
