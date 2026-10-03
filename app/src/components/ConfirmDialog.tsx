// ConfirmDialog — a small, reusable modal for confirming a Cockpit write (#213).
//
// The Cockpit's first write actions are human-operated: a person clicks an
// action, the modal states what will happen, the person confirms (optionally
// adding a note), and the write runs. This component owns the modal chrome,
// the busy state and inline error/lock rendering so each call site stays a
// thin prop bag.
//
// Shared on purpose — slices 2-4 (bank import, document intake, invoicing)
// reuse it. The `confirmKind` prop lets a destructive action render a danger
// button; slice 1's resolve-exception action is non-destructive.

import { useContext, useRef, useState } from "react";
import { UNSAFE_LocationContext } from "react-router-dom";
import { useMutationBlock } from "../lib/mutation-memory";
import { UnknownMutationNotice } from "./UnknownMutationNotice";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";
import { Button, Dialog, Input, Textarea } from "./ui";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

export type ConfirmDialogProps = {
  /** Modal heading. */
  title: string;
  /** Body text describing what the confirm will do. */
  body: React.ReactNode;
  /** Label for the confirm button. */
  confirmLabel: string;
  /** Confirm button tone. Defaults to the primary button. */
  confirmKind?: "primary" | "danger";
  /** When set, a note field is shown and its value passed to `onConfirm`. */
  noteLabel?: string;
  /** Placeholder for the note field. */
  notePlaceholder?: string;
  /**
   * Optional initial value for the note field — used by #429's "Send på mail"
   * dialog to prefill the recipient with the customer's stored e-mail while
   * still letting the owner override it before sending.
   */
  noteInitialValue?: string;
  /**
   * When `"email"` the note field renders as a single-line `<Input
   * type="email">` instead of the default `<Textarea>` — used by #429 so the
   * cockpit gets browser-native e-mail validation on the recipient field.
   */
  noteInputType?: "textarea" | "email";
  /**
   * Runs the write. Resolves on success (the dialog then closes via `onClose`);
   * rejects to surface an error. A rejection carrying `code === "conflict"` is
   * rendered as a kind LockBanner — the backup lock, not a user error.
   */
  onConfirm: (note: string) => Promise<void>;
  /** Closes the dialog without acting. */
  onClose: () => void;
  /** A read-only refresh after a transport failure; never repeats the write. */
  onRefresh?: () => unknown | Promise<unknown>;
  /** Stable identity within the route, e.g. invoice document id. */
  operationKey?: string;
  closeOnConfirm?: boolean;
};

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

export function ConfirmDialog({
  title,
  body,
  confirmLabel,
  confirmKind = "primary",
  noteLabel,
  notePlaceholder,
  noteInitialValue,
  noteInputType = "textarea",
  onConfirm,
  onClose,
  onRefresh,
  operationKey = "",
  closeOnConfirm = true,
}: ConfirmDialogProps) {
  const [note, setNote] = useState(noteInitialValue ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const location = useContext(UNSAFE_LocationContext)?.location;
  const key = `${location?.pathname ?? ""}:${title}:${operationKey}`;
  const outcome = useMutationBlock(key);
  const uncertain = outcome.blocked && !busy;
  const attempted = useRef(false);
  const [checkingDiscard, setCheckingDiscard] = useState(false);
  const [completed, setCompleted] = useState(false);
  const dirty = Boolean(noteLabel) && !completed && note !== (noteInitialValue ?? "");
  const markSaved = useUnsavedChanges(dirty);
  const [locked, setLocked] = useState<string | null>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const noteRef = useRef<HTMLInputElement>(null);
  function requestClose() {
    if (busy) return;
    if (dirty) setCheckingDiscard(true); else onClose();
  }
  async function handleConfirm() {
    if (attempted.current || outcome.isBlocked()) return;
    if (noteInputType === "email" && noteRef.current && !noteRef.current.reportValidity()) return;
    if (!outcome.begin()) return;
    attempted.current = true;
    setBusy(true);
    setError(null);
    setLocked(null);
    try {
      await onConfirm(note.trim());
      outcome.release();
      setCompleted(true); markSaved();
      if (closeOnConfirm) onClose();
    } catch (err) {
      const e = err as MaybeApiError;
      if (e?.code === "network" || e?.code === "internal") outcome.block();
      else outcome.release();
      const message = e?.message ?? "Handlingen kunne ikke gennemføres.";
      // A 409 conflict from the backup lock is shown kindly, not as an error.
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
      setBusy(false);
    } finally { attempted.current = false; }
  }

  return (
    <Dialog title={title} onClose={requestClose} busy={busy} initialFocusRef={confirmKind === "danger" ? cancelRef : confirmRef}>
        {checkingDiscard && <ConfirmDialog title="Kassér ændringer?" body="Du har ændringer, som ikke er gemt. Hvis du lukker formularen, bliver de kasseret." confirmLabel="Kassér ændringer" confirmKind="danger" onClose={() => setCheckingDiscard(false)} onConfirm={async () => { markSaved(); setCompleted(true); onClose(); }} />}
        <div className="modal-body">{body}</div>

        {locked && <LockBanner message={locked} />}
        {error && <Banner kind="error">{error}</Banner>}
        {uncertain && <UnknownMutationNotice onRefresh={onRefresh} onRelease={() => { outcome.release(); setError(null); }} persistent={outcome.persistent} verifyPersistence={outcome.verifyPersistence} />}

        {noteLabel && (
          <label className="modal-field">
            {noteLabel}
            {noteInputType === "email" ? (
              <Input
                ref={noteRef}
                type="email"
                value={note}
                placeholder={notePlaceholder}
                onChange={(e) => setNote(e.target.value)}
                disabled={busy || uncertain}
              />
            ) : (
              <Textarea
                value={note}
                placeholder={notePlaceholder}
                onChange={(e) => setNote(e.target.value)}
                disabled={busy || uncertain}
                rows={3}
              />
            )}
          </label>
        )}

        <div className="modal-actions">
          <Button
            ref={cancelRef}
            type="button"
            className="btn secondary"
            onClick={requestClose}
            disabled={busy}
          >
            Annullér
          </Button>
          <Button
            ref={confirmRef}
            type="button"
            className={`btn${confirmKind === "danger" ? " danger" : ""}`}
            onClick={handleConfirm}
            disabled={busy || uncertain}
          >
            {busy ? "Arbejder…" : confirmLabel}
          </Button>
        </div>
    </Dialog>
  );
}
