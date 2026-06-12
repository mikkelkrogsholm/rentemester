// ImportModal — the cockpit's generic, source-recognising file-import.
//
// A person migrating from another accounting system picks an export file; the
// browser reads its text and POSTs it. The server recognises which system the
// file came from (`detectImportSource`) and routes it to the matching core
// importer. The modal owns the file picker, the CVR-enrich opt-in, the busy
// state and the post-import receipt — it mirrors `BankImportModal`'s shape.

import { useEffect, useRef, useState } from "react";
import { api, type DataImportSummary } from "../lib/api";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

export type ImportModalProps = {
  /** Company slug the import targets. */
  slug: string;
  /** Re-runs the calling view's load after a successful import. */
  onImported: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

export function ImportModal({ slug, onImported, onClose }: ImportModalProps) {
  const [fileName, setFileName] = useState<string | null>(null);
  const [content, setContent] = useState<string | null>(null);
  const [enrichCvr, setEnrichCvr] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<DataImportSummary | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  // Move focus into the dialog and let Escape dismiss it — basic modal hygiene.
  useEffect(() => {
    closeRef.current?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  async function handleFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    setError(null);
    if (!file) {
      setFileName(null);
      setContent(null);
      return;
    }
    try {
      const text = await file.text();
      setFileName(file.name);
      setContent(text);
    } catch {
      setError("Filen kunne ikke læses.");
      setFileName(null);
      setContent(null);
    }
  }

  async function handleImport() {
    if (!content || !fileName) {
      setError("Vælg en fil først.");
      return;
    }
    setBusy(true);
    setError(null);
    setLocked(null);
    try {
      const summary = await api.importData(slug, {
        fileName,
        content,
        enrichCvr,
      });
      setDone(summary);
      onImported();
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Importen kunne ikke gennemføres.";
      // A 409 conflict from the backup lock is shown kindly, not as an error.
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
    } finally {
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
        aria-label="Importér fil"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="modal-title">Importér fil</h3>

        {done ? (
          <ImportReceipt done={done} closeRef={closeRef} onClose={onClose} />
        ) : (
          <>
            <div className="modal-body">
              <p>
                Vælg en eksportfil fra dit tidligere bogføringssystem —
                Rentemester genkender selv formatet. Understøttet nu: Dinero
                «Kontakter» (Kontakter.csv) med kunder og leverandører.
              </p>
            </div>

            {locked && <LockBanner message={locked} />}
            {error && <Banner kind="error">{error}</Banner>}

            <label className="modal-field">
              Fil
              <input
                type="file"
                accept=".csv,text/csv"
                onChange={handleFile}
                disabled={busy}
              />
            </label>
            {fileName && (
              <p className="muted" style={{ margin: 0 }}>
                Valgt: {fileName}
              </p>
            )}

            <label className="modal-field modal-checkbox">
              <input
                type="checkbox"
                checked={enrichCvr}
                onChange={(e) => setEnrichCvr(e.target.checked)}
                disabled={busy}
              />
              Berig danske virksomheder med adresse m.m. fra CVR-registeret
            </label>

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
                type="button"
                className="btn"
                onClick={handleImport}
                disabled={busy || !content}
              >
                {busy ? "Importerer…" : "Importér"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

// After a successful import the modal becomes a short receipt: what the file
// was recognised as, and how many contacts were created / skipped / enriched.
function ImportReceipt({
  done,
  closeRef,
  onClose,
}: {
  done: DataImportSummary;
  closeRef: React.RefObject<HTMLButtonElement>;
  onClose: () => void;
}) {
  const s = done.summary;
  const created = s.customersCreated + s.vendorsCreated;
  // CVR enrichment degrades gracefully — a failure because the CVR register
  // credentials are unset is a setup gap, not an import error, so it gets a
  // calm, specific note rather than the raw per-contact failure list.
  const cvrCredsMissing = done.errors.some((e) => e.includes("CVR_USERNAME"));
  // Errors that are NOT CVR enrichment — skipped rows, rows that could not be
  // created. The summary counts hide these, so they must be surfaced or the
  // owner sees "X oprettet" with no hint that rows were dropped.
  const otherErrors = done.errors.filter((e) => !e.includes("CVR-berigelse"));

  return (
    <>
      <div className="modal-body">
        <p>
          {done.detected
            ? `Genkendt som ${done.detected.label}.`
            : "Filen blev importeret."}
        </p>
        <p>
          {created} {created === 1 ? "kontakt" : "kontakter"} oprettet (
          {s.customersCreated}{" "}
          {s.customersCreated === 1 ? "kunde" : "kunder"},{" "}
          {s.vendorsCreated}{" "}
          {s.vendorsCreated === 1 ? "leverandør" : "leverandører"})
          {s.skipped > 0 ? ` · ${s.skipped} fandtes allerede` : ""}
          {s.enriched > 0 ? ` · ${s.enriched} beriget fra CVR` : ""}.
        </p>
      </div>

      {cvrCredsMissing && (
        <Banner kind="warning">
          CVR-berigelse blev ikke kørt — adgang til CVR-registeret er ikke
          konfigureret. Sæt CVR_USERNAME og CVR_PASSWORD for at hente adresser
          m.m. automatisk.
        </Banner>
      )}
      {!cvrCredsMissing && s.enrichmentFailures > 0 && (
        <Banner kind="warning">
          CVR-berigelse fejlede for {s.enrichmentFailures}{" "}
          {s.enrichmentFailures === 1 ? "kontakt" : "kontakter"} — de er
          oprettet med dataene fra filen.
        </Banner>
      )}
      {otherErrors.length > 0 && (
        <Banner kind="warning">
          {otherErrors.length === 1
            ? otherErrors[0]
            : `${otherErrors.length} rækker kunne ikke importeres: ` +
              otherErrors.slice(0, 3).join("; ") +
              (otherErrors.length > 3 ? " …" : "")}
        </Banner>
      )}

      <div className="modal-actions">
        <button
          type="button"
          className="btn"
          ref={closeRef}
          onClick={onClose}
        >
          Luk
        </button>
      </div>
    </>
  );
}
