import { createContext, useContext, useEffect, useMemo, useState, useSyncExternalStore, type ReactNode } from "react";

const LEGACY_STORAGE_KEY = "rentemester:uncertain-operations:v1";
const LEGACY_PREFIX = "rentemester:uncertain-operations:v2:legacy:";
const CHANGE_EVENT = "rentemester:mutation-memory-changed";
type MutationMemory = {
  has: (key: string) => boolean;
  block: (key: string) => void;
  release: (key: string) => void;
  persistent: () => boolean;
  verifyPersistence: () => void;
  subscribe: (listener: () => void) => () => void;
  notify: () => void;
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

/** Interrupted writes remain blocked in every tab for this origin and user. */
export function MutationMemoryProvider({ children, scope = "local" }: { children: ReactNode; scope?: string }) {
  const prefix = `rentemester:uncertain-operations:v2:${identifier(scope)}:`;
  const memory = useMemo<MutationMemory>(() => {
    let persistent = false;
    const pending = new Set<string>();
    const listeners = new Set<() => void>();
    function notify() { for (const listener of listeners) listener(); }
    function verify() {
      try {
        // Each operation owns a key, so concurrent tabs cannot overwrite
        // unrelated blockers with an out-of-date array.
        localStorage.setItem(`${prefix}probe`, "1");
        if (localStorage.getItem(`${prefix}probe`) !== "1") throw new Error("storage verification failed");
        const legacy = sessionStorage.getItem(LEGACY_STORAGE_KEY);
        if (legacy !== null) {
          const saved: unknown = JSON.parse(legacy);
          if (!Array.isArray(saved) || saved.some(item => typeof item !== "string")) throw new Error("invalid legacy blockers");
          // v1 had no authenticated owner. Preserve a shared quarantine;
          // never transfer an unknown operation to the first user to log in.
          for (const id of saved) {
            localStorage.setItem(`${LEGACY_PREFIX}${id}`, "1");
            if (localStorage.getItem(`${LEGACY_PREFIX}${id}`) !== "1") throw new Error("legacy blocker verification failed");
          }
        }
        for (const id of pending) {
          localStorage.setItem(`${prefix}${id}`, "1");
          if (localStorage.getItem(`${prefix}${id}`) !== "1") throw new Error("blocker verification failed");
        }
        if (legacy !== null) sessionStorage.removeItem(LEGACY_STORAGE_KEY);
        pending.clear();
        persistent = true;
      } catch { persistent = false; }
    }
    verify();
    function update(key: string, shouldBlock: boolean) {
      const id = identifier(key);
      if (shouldBlock) pending.add(id);
      if (!persistent) { notify(); return; }
      try {
        if (shouldBlock) localStorage.setItem(`${prefix}${id}`, "1");
        else {
          localStorage.removeItem(`${prefix}${id}`);
          localStorage.removeItem(`${LEGACY_PREFIX}${id}`);
        }
        if ((localStorage.getItem(`${prefix}${id}`) !== null) !== shouldBlock) throw new Error("blocker verification failed");
        pending.delete(id);
      } catch { persistent = false; }
      notify();
      // Native storage events reach other tabs; this event reaches sibling
      // providers in the same document without writing storage again.
      window.dispatchEvent(new Event(CHANGE_EVENT));
    }
    return {
      has: key => {
        if (!persistent) return true;
        try { return pending.has(identifier(key)) || localStorage.getItem(`${prefix}${identifier(key)}`) !== null || localStorage.getItem(`${LEGACY_PREFIX}${identifier(key)}`) !== null; }
        catch { persistent = false; return true; }
      },
      block: key => update(key, true),
      release: key => update(key, false),
      persistent: () => persistent,
      verifyPersistence: () => { verify(); notify(); },
      subscribe: listener => { listeners.add(listener); return () => { listeners.delete(listener); }; },
      notify,
    };
  }, [prefix]);
  useEffect(() => {
    const storageChanged = (event: StorageEvent) => { if (event.key === null || (event.key.startsWith(prefix) || event.key.startsWith(LEGACY_PREFIX))) memory.notify(); };
    const locallyChanged = () => memory.notify();
    window.addEventListener("storage", storageChanged);
    window.addEventListener(CHANGE_EVENT, locallyChanged);
    return () => { window.removeEventListener("storage", storageChanged); window.removeEventListener(CHANGE_EVENT, locallyChanged); };
  }, [memory, prefix]);
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
    begin: () => {
      if (local || memory?.has(key)) return false;
      if (!memory) { setLocal(true); return true; }
      memory.block(key);
      return memory.persistent();
    },
    block: () => { if (memory) memory.block(key); else setLocal(true); },
    release: () => { if (memory) memory.release(key); else setLocal(false); },
  };
}
