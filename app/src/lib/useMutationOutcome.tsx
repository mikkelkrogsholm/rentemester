import { useContext } from "react";
import { UNSAFE_LocationContext } from "react-router-dom";
import { UnknownMutationNotice } from "../components/UnknownMutationNotice";
import { useMutationBlock } from "./mutation-memory";

/** An interrupted write is never safe to repeat merely because a read succeeded. */
export function useMutationOutcome(onRefresh?: () => unknown | Promise<unknown>, operationKey = "form", scopeKey?: string) {
  const location = useContext(UNSAFE_LocationContext)?.location;
  // Query filters and explicit/default year aliases must not bypass a lock.
  const block = useMutationBlock(`${scopeKey ?? location?.pathname ?? ""}:outcome:${operationKey}`);
  function reject(error: unknown): never {
    const code = error && typeof error === "object" && "code" in error ? error.code : undefined;
    if (code === "network" || code === "internal") {
      block.block();
    }
    throw error;
  }
  const feedback = block.blocked ? <UnknownMutationNotice onRefresh={onRefresh} onRelease={block.release} persistent={block.persistent} verifyPersistence={block.verifyPersistence} /> : null;
  return { blocked: block.blocked, isBlocked: block.isBlocked, reject, feedback };
}
