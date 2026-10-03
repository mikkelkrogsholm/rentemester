import { createContext, useCallback, useContext, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react';
import { useBlocker } from 'react-router-dom';
import { ConfirmDialog } from '../components/ConfirmDialog';

type Register = (id: string, dirty: boolean) => void;
const PendingContext = createContext(false);
export function usePendingChanges() { return useContext(PendingContext); }
const UnsavedContext = createContext<Register | null>(null);
/** Root lives under RouterProvider. Forms rendered alone still protect browser unload. */
export function UnsavedChangesProvider({ children }: { children: ReactNode }) {
  const [dirtyForms, setDirtyForms] = useState<ReadonlySet<string>>(new Set());
  const currentDirty = useRef(new Set<string>());
  const register = useCallback<Register>((id, dirty) => {
    if (currentDirty.current.has(id) === dirty) return;
    if (dirty) currentDirty.current.add(id); else currentDirty.current.delete(id);
    setDirtyForms(new Set(currentDirty.current));
  }, []);
  const blocker = useBlocker(({ currentLocation, nextLocation }) => currentDirty.current.size > 0 && (currentLocation.pathname !== nextLocation.pathname || currentLocation.search !== nextLocation.search));
  const value = useMemo(() => register, [register]);
  return <PendingContext.Provider value={dirtyForms.size > 0}><UnsavedContext.Provider value={value}>{children}{blocker.state === 'blocked' && <ConfirmDialog title="Forlad siden?" body="Du har ændringer, som ikke er gemt. Hvis du forlader siden, bliver de kasseret." confirmLabel="Forlad siden" confirmKind="danger" closeOnConfirm={false} onClose={() => blocker.reset()} onConfirm={async () => blocker.proceed()} />}</UnsavedContext.Provider></PendingContext.Provider>;
}
export function useUnsavedChanges(dirty: boolean) {
  const register = useContext(UnsavedContext);
  const id = useId();
  useEffect(() => {
    register?.(id, dirty);
    return () => register?.(id, false);
  }, [register, id, dirty]);
  useEffect(() => {
    if (!dirty) return;
    const unload = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ''; };
    window.addEventListener('beforeunload', unload);
    return () => window.removeEventListener('beforeunload', unload);
  }, [dirty]);
  return () => register?.(id, false);
}
