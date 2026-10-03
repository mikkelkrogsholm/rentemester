import { useContext, useState } from "react";
import { UNSAFE_LocationContext } from "react-router-dom";
import { UnknownMutationNotice } from "../components/UnknownMutationNotice";
import { useMutationBlock } from "./mutation-memory";

/** An interrupted write is never safe to repeat merely because a read succeeded. */
export function useMutationOutcome(onRefresh?: () => unknown | Promise<unknown>, operationKey = "form", scopeKey?: string) {
  const location = useContext(UNSAFE_LocationContext)?.location;
  // Query filters and explicit/default year aliases must not bypass a lock.
  const block = useMutationBlock(`${scopeKey ?? location?.pathname ?? ""}:outcome:${operationKey}`);
  const [running, setRunning] = useState(false);
  function reject(error: unknown): never {
    const code = error && typeof error === "object" && "code" in error ? error.code : undefined;
    if (code === "network" || code === "internal") {
      block.block();
    }
    throw error;
  }
  async function run<T>(write: () => Promise<T>): Promise<T> {
    setRunning(true);
    if (!block.begin()) { setRunning(false); throw new Error("Skrivningen er blokeret. Kontrollér resultatet og browserens lager."); }
    try {
      const result = await write();
      block.release();
      return result;
    } catch (error) {
      const code = error && typeof error === "object" && "code" in error ? error.code : undefined;
      // The durable pre-request blocker stays for unknown results. A known
      // rejection is safe to correct; a success is safe to leave.
      if (code !== "network" && code !== "internal") block.release();
      throw error;
    } finally { setRunning(false); }
  }
  const feedback = block.blocked && !running ? <UnknownMutationNotice onRefresh={onRefresh} onRelease={block.release} persistent={block.persistent} verifyPersistence={block.verifyPersistence} /> : null;
  return { blocked: block.blocked, isBlocked: block.isBlocked, reject, run, feedback };
}
