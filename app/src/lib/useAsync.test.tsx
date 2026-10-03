import { describe, expect, it } from "bun:test";
import { act, renderHook, waitFor } from "@testing-library/react";
import { ApiError } from "./api/_shared";
import { useAsync } from "./useAsync";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((onResolve, onReject) => { resolve = onResolve; reject = onReject; });
  return { promise, resolve, reject };
}

describe("useAsync resource identity", () => {
  it("hides previous-company data in the very first render of a new company", async () => {
    const alpha = deferred<string>();
    const beta = deferred<string>();
    const renders: Array<{ slug: string; data: string | null }> = [];
    const { result, rerender } = renderHook(({ slug }) => {
      const resource = useAsync(() => slug === "alpha" ? alpha.promise : beta.promise, [slug]);
      renders.push({ slug, data: resource.data });
      return resource;
    }, { initialProps: { slug: "alpha" } });
    await act(async () => alpha.resolve("alpha ledger"));
    expect(result.current.data).toBe("alpha ledger");
    rerender({ slug: "beta" });
    expect(renders.find(render => render.slug === "beta")?.data).toBeNull();
    expect(result.current.data).toBeNull();
    expect(result.current.loading).toBe(true);
    await act(async () => beta.resolve("beta ledger"));
    expect(result.current.data).toBe("beta ledger");
  });

  it("suppresses an older response arriving after the new resource", async () => {
    const older = deferred<string>();
    const newer = deferred<string>();
    const { result, rerender } = renderHook(({ year }) => useAsync(
      () => year === "2025" ? older.promise : newer.promise, [year],
    ), { initialProps: { year: "2025" } });
    rerender({ year: "2026" });
    await act(async () => newer.resolve("2026"));
    await act(async () => older.resolve("2025"));
    expect(result.current.data).toBe("2026");
    expect(result.current.loading).toBe(false);
  });

  it("treats an explicit resourceKey change as a new resource", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    const { result, rerender } = renderHook(({ key }) => useAsync(
      () => key === "one" ? first.promise : second.promise, [], { resourceKey: key },
    ), { initialProps: { key: "one" } });
    await act(async () => first.resolve("first"));
    rerender({ key: "two" });
    expect(result.current.data).toBeNull();
    await act(async () => second.resolve("second"));
    expect(result.current.data).toBe("second");
  });

  it("keeps data during same-resource reload and preserves structured failure", async () => {
    const first = deferred<string>();
    const second = deferred<string>();
    let calls = 0;
    const { result } = renderHook(() => useAsync(() => calls++ === 0 ? first.promise : second.promise, []));
    await act(async () => first.resolve("current"));
    act(() => result.current.reload());
    expect(result.current.data).toBe("current");
    expect(result.current.refreshing).toBe(true);
    const failure = new ApiError("conflict", "Planen er ændret.", 409);
    await act(async () => second.reject(failure));
    expect(result.current.data).toBe("current");
    expect(result.current.failure).toBe(failure);
    expect(result.current.error).toBe("Planen er ændret.");
    expect(result.current.loading).toBe(false);
    expect(result.current.refreshing).toBe(false);
  });

  it("aborts only superseded reads and unmounted reads", async () => {
    const signals: AbortSignal[] = [];
    const pending = deferred<string>();
    const { result, rerender, unmount } = renderHook(({ slug }) => useAsync(signal => {
      signals.push(signal);
      return pending.promise;
    }, [slug]), { initialProps: { slug: "first" } });
    await waitFor(() => expect(signals).toHaveLength(1));
    rerender({ slug: "second" });
    await waitFor(() => expect(signals).toHaveLength(2));
    expect(signals[0]?.aborted).toBe(true);
    expect(signals[1]?.aborted).toBe(false);
    expect(result.current.error).toBeNull();
    unmount();
    expect(signals[1]?.aborted).toBe(true);
    await act(async () => pending.resolve("late"));
  });

  it("clears an old error immediately on a resource change", async () => {
    const next = deferred<string>();
    const { result, rerender } = renderHook(({ slug }) => useAsync(
      () => slug === "denied" ? Promise.reject(new ApiError("forbidden", "Ingen adgang.", 403)) : next.promise,
      [slug],
    ), { initialProps: { slug: "denied" } });
    await waitFor(() => expect(result.current.error).toBe("Ingen adgang."));
    rerender({ slug: "allowed" });
    expect(result.current.error).toBeNull();
    expect(result.current.failure).toBeNull();
    await act(async () => next.resolve("allowed"));
    expect(result.current.data).toBe("allowed");
  });
});
