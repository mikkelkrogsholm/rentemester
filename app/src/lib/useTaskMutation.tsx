import { useState } from 'react';
import { useLocation } from 'react-router-dom';
import { Button } from '../components/ui';
import { api, ApiError } from './api';
import { useOptionalAuth } from './auth-context';
import { useMutationOutcome } from './useMutationOutcome';

/** Persist only an opaque operations key; task contents never enter browser storage. */
export function useTaskMutation(onSaved: () => void, operation: string, onVerified?: () => void) {
  const location = useLocation();
  const auth = useOptionalAuth();
  const storageKey = `rentemester:task-write:${auth?.session?.id ?? 'local'}:${location.pathname}:${operation}`;
  const [pending, setPending] = useState<string | null>(() => { try { return sessionStorage.getItem(storageKey); } catch { return null; } });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [verified, setVerified] = useState(false);
  const outcome = useMutationOutcome(onSaved, operation);
  function clear() { setPending(null); try { sessionStorage.removeItem(storageKey); } catch { /* The durable outcome lock remains authoritative. */ } }
  async function run<T>(path: string, values: Record<string, unknown>): Promise<T | undefined> {
    if (busy || outcome.isBlocked()) return;
    const idempotencyKey = crypto.randomUUID();
    setBusy(true); setError(null); setVerified(false); setPending(idempotencyKey);
    try {
      sessionStorage.setItem(storageKey, idempotencyKey);
      const result = await outcome.run(() => api.taskWrite<T>(path, { ...values, idempotencyKey, confirm: true }));
      clear(); onSaved(); return result;
    } catch (cause) {
      if (cause instanceof ApiError && cause.status === 409) onSaved();
      setError(cause instanceof ApiError && cause.status === 409 ? `${cause.message} Din indtastning er bevaret. Hent den aktuelle version, og kontrollér ændringen før du gemmer igen.` : cause instanceof Error ? cause.message : 'Handlingen kunne ikke gennemføres.');
      if (!(cause instanceof ApiError) || !['network', 'internal'].includes(cause.code)) clear();
      return undefined;
    } finally { setBusy(false); }
  }
  async function verify() {
    if (!pending || busy) return;
    setBusy(true); setError(null);
    try {
      const result = await api.taskOperationReceipt(pending);
      if (!result.receipt) { setError('Der er ingen verificeret kvittering endnu. Resultatet er fortsat ukendt; start ikke en ny handling.'); return; }
      outcome.acceptVerifiedResult(); clear(); setVerified(true); onSaved(); onVerified?.();
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Resultatet er fortsat ukendt.'); }
    finally { setBusy(false); }
  }
  return { run, busy, blocked: outcome.blocked, error, feedback: <>
    {outcome.feedback}
    {outcome.blocked && pending && <Button variant="secondary" busy={busy} onClick={() => void verify()}>Verificér samme handling</Button>}
    {error && <p className="banner warning" role="alert">{error}</p>}
    {verified && <p className="banner success" role="status">Handlingen er verificeret i operationskvitteringen.</p>}
  </> };
}
