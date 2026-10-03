import { useCallback, useEffect, useRef, useState } from "react";

export type AsyncState<T> = {
  data: T | null;
  loading: boolean;
  /** A reload of the same resource can keep its previous data visible. */
  refreshing: boolean;
  error: string | null;
  /** The original structured API error, when available. */
  failure: Error | null;
  /** Re-runs the loader; used after a mutation invalidates the data. */
  reload: () => void;
};

export type AsyncOptions = {
  /** An additional identity for loaders whose resource is not fully in deps. */
  resourceKey?: unknown;
};

type Snapshot<T> = {
  deps: readonly unknown[];
  resourceKey: unknown;
  tick: number;
  data: T | null;
  loading: boolean;
  failure: Error | null;
};

function sameResource<T>(snapshot: Snapshot<T>, deps: readonly unknown[], resourceKey: unknown): boolean {
  return Object.is(snapshot.resourceKey, resourceKey) && snapshot.deps.length === deps.length &&
    snapshot.deps.every((value, index) => Object.is(value, deps[index]));
}

/**
 * A small, uncached read state machine. Resource identity is checked during
 * render so the previous company's data is never exposed while effects catch
 * up. Cancellation is local to a read; ledger writes are not retried or aborted.
 */
export function useAsync<T>(
  loader: (signal: AbortSignal) => Promise<T>,
  deps: readonly unknown[],
  options: AsyncOptions = {},
): AsyncState<T> {
  const resourceKey = options.resourceKey;
  const [tick, setTick] = useState(0);
  const [snapshot, setSnapshot] = useState<Snapshot<T>>(() => ({
    deps: [...deps], resourceKey, tick, data: null, loading: true, failure: null,
  }));
  const loaderRef = useRef(loader);
  loaderRef.current = loader;

  // biome-ignore lint/correctness/useExhaustiveDependencies: compare caller dependency values, not the newly allocated array itself.
  useEffect(() => {
    let cancelled = false;
    const controller = new AbortController();
    const identity = { deps: [...deps], resourceKey, tick };
    setSnapshot(previous => ({
      ...identity,
      data: sameResource(previous, deps, resourceKey) ? previous.data : null,
      loading: true,
      failure: null,
    }));

    const load = loaderRef.current;
    // The async boundary also normalizes a synchronous loader error.
    void (async () => {
      if (cancelled) return;
      const result = await load(controller.signal);
      if (!cancelled) setSnapshot({ ...identity, data: result, loading: false, failure: null });
    })().catch((cause: unknown) => {
      if (!cancelled) {
        const failure = cause instanceof Error ? cause : new Error(String(cause));
        setSnapshot(previous => ({
          ...identity,
          data: sameResource(previous, deps, resourceKey) ? previous.data : null,
          loading: false,
          failure,
        }));
      }
    });
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [...deps, resourceKey, tick]);

  const reload = useCallback(() => setTick(value => value + 1), []);
  const matches = sameResource(snapshot, deps, resourceKey);
  const data = matches ? snapshot.data : null;
  const loading = !matches || snapshot.tick !== tick || snapshot.loading;
  const failure = matches && snapshot.tick === tick ? snapshot.failure : null;
  return { data, loading, refreshing: loading && data !== null, error: failure?.message ?? null, failure, reload };
}
