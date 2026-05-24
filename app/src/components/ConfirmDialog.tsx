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

import { useEffect, useRef, useState } from "react";
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
   * When `"email"` the note field renders as a single-line `<input
   * type="email">` instead of the default `<textarea>` — used by #429 so the
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
}: ConfirmDialogProps) {
  const [note, setNote] = useState(noteInitialValue ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  // Move focus into the dialog and let Escape dismiss it — basic modal hygiene.
  useEffect(() => {
    confirmRef.current?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  async function handleConfirm() {
    setBusy(true);
    setError(null);
    setLocked(null);
    try {
      await onConfirm(note.trim());
      onClose();
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Handlingen kunne ikke gennemføres.";
      // A 409 conflict from the backup lock is shown kindly, not as an error.
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
      setBusy(false);
    }
  }

  return (
    <div
      className="modal-overlay"
      role="presentation"
      onClick={() => {
        if (!busy) onClose();
      }}
    >
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="modal-title">{title}</h3>
        <div className="modal-body">{body}</div>

        {locked && <LockBanner message={locked} />}
        {error && <Banner kind="error">{error}</Banner>}

        {noteLabel && (
          <label className="modal-field">
            {noteLabel}
            {noteInputType === "email" ? (
              <input
                type="email"
                value={note}
                placeholder={notePlaceholder}
                onChange={(e) => setNote(e.target.value)}
                disabled={busy}
              />
            ) : (
              <textarea
                value={note}
                placeholder={notePlaceholder}
                onChange={(e) => setNote(e.target.value)}
                disabled={busy}
                rows={3}
              />
            )}
          </label>
        )}

        <div className="modal-actions">
          <button
            type="button"
            className="btn secondary"
            onClick={onClose}
            disabled={busy}
          >
            Annullér
          </button>
          <button
            ref={confirmRef}
            type="button"
            className={`btn${confirmKind === "danger" ? " danger" : ""}`}
            onClick={handleConfirm}
            disabled={busy}
          >
            {busy ? "Arbejder…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
