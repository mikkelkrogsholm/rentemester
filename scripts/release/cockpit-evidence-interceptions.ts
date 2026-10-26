/** Network.loadingFailed uses Network.RequestId, not Fetch.RequestId. */
export function cancelledNetworkRequestId(params: unknown): string | undefined {
  if (!params || typeof params !== "object") return undefined;
  const failure = params as { requestId?: unknown; canceled?: unknown };
  return failure.canceled === true && typeof failure.requestId === "string" && failure.requestId.length > 0
    ? failure.requestId
    : undefined;
}

/**
 * Attach rejection handling when an interception starts, rather than waiting
 * until the scenario finishes. Chrome invalidates paused Fetch ids when the
 * application aborts a read. Accept that one protocol error only when Chrome
 * also reported cancellation of this paused request's exact Network id.
 * All other failures remain release blockers with scenario/request context.
 */
export async function settlePausedRequest(options: {
  scenario: string;
  requestId: string;
  networkId?: string;
  url?: string;
  cancelledNetworkRequests: ReadonlySet<string>;
  operation: () => Promise<unknown>;
}): Promise<Error | undefined> {
  try {
    await options.operation();
    return undefined;
  } catch (cause) {
    if (
      cause instanceof Error &&
      /^Chrome protocol error \(Fetch\.(fulfillRequest|continueRequest)\): Invalid InterceptionId\.$/.test(cause.message) &&
      options.networkId !== undefined &&
      options.cancelledNetworkRequests.has(options.networkId)
    ) return undefined;
    return new Error(
      `${options.scenario} interception failed: ${options.url ?? "unknown URL"} (Fetch id ${options.requestId}, Network id ${options.networkId ?? "missing"}): ${String(cause)}`,
      { cause },
    );
  }
}

/** Also drain requests appended while earlier delayed interceptions settle. */
export async function assertSettledInterceptions(interceptions: Promise<Error | undefined>[]): Promise<void> {
  let drained = 0;
  while (drained < interceptions.length) {
    const batch = interceptions.slice(drained);
    drained += batch.length;
    const failure = (await Promise.all(batch)).find((error) => error !== undefined);
    if (failure) throw failure;
  }
}
