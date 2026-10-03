import { createContext, useContext, useMemo, useState, useSyncExternalStore, type ReactNode } from "react";

const STORAGE_KEY = "rentemester:uncertain-operations:v1";
type MutationMemory = {
  has: (key: string) => boolean;
  block: (key: string) => void;
  release: (key: string) => void;
  persistent: () => boolean;
  verifyPersistence: () => void;
  subscribe: (listener: () => void) => () => void;
};
const Context = createContext<MutationMemory | null>(null);

// Storage contains opaque operation identifiers, never form values or subjects.
function identifier(value: string) {
  let a = 2166136261; let b = 5381;
  for (const char of value) {
    a = Math.imul(a ^ char.charCodeAt(0), 16777619);
    b = Math.imul(b, 33) ^ char.charCodeAt(0);
  }
  return `${a >>> 0}:${b >>> 0}`;
}

/** Interrupted writes remain blocked across route changes and tab reloads. */
export function MutationMemoryProvider({ children }: { children: ReactNode }) {
  const memory = useMemo<MutationMemory>(() => {
    let saved: string[] = [];
    let persistent = true;
    try {
      const value: unknown = JSON.parse(sessionStorage.getItem(STORAGE_KEY) ?? "[]");
      if (Array.isArray(value)) saved = value.filter((item): item is string => typeof item === "string");
    } catch { persistent = false; }
    const blocked = new Set(saved);
    const listeners = new Set<() => void>();
    function persist() {
      try {
        const serialized = JSON.stringify([...blocked]);
        sessionStorage.setItem(STORAGE_KEY, serialized);
        persistent = sessionStorage.getItem(STORAGE_KEY) === serialized;
      } catch { persistent = false; }
      for (const listener of listeners) listener();
    }
    // Fail closed before the first write if the browser denies session storage.
    if (persistent) persist();
    function update(key: string, shouldBlock: boolean) {
      if (!shouldBlock && !persistent) return;
      if (shouldBlock) blocked.add(identifier(key)); else blocked.delete(identifier(key));
      persist();
    }
    return {
      has: (key) => !persistent || blocked.has(identifier(key)),
      block: (key) => update(key, true),
      release: (key) => update(key, false),
      persistent: () => persistent,
      verifyPersistence: persist,
      subscribe: (listener) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    };
  }, []);
  return <Context.Provider value={memory}>{children}</Context.Provider>;
}

const noSubscription = () => () => {};
const unblocked = () => false;
const storageReady = () => true;

export function useMutationBlock(key: string) {
  const memory = useContext(Context);
  const [local, setLocal] = useState(false);
  const remembered = useSyncExternalStore(memory?.subscribe ?? noSubscription, () => memory?.has(key) ?? false, unblocked);
  const persistent = useSyncExternalStore(memory?.subscribe ?? noSubscription, () => memory?.persistent() ?? true, storageReady);
  return {
    blocked: local || remembered,
    isBlocked: () => local || memory?.has(key) === true,
    persistent,
    verifyPersistence: memory?.verifyPersistence,
    block: () => { memory?.block(key); setLocal(true); },
    release: () => { memory?.release(key); setLocal(false); },
  };
}
