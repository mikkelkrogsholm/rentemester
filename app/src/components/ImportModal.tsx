import { cockpitStyles } from "../design/cockpit.stylex";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import * as stylex from "@stylexjs/stylex";
import { Button, Dialog, Input } from "./ui";
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

export function ImportModal(props: ImportModalProps) {
  return <CompanyImportModal key={props.slug} {...props} />;
}

function CompanyImportModal({ slug, onImported, onClose: onDismiss }: ImportModalProps) {
  const [fileName, setFileName] = useState<string | null>(null);
  const [content, setContent] = useState<string | null>(null);
  const [enrichCvr, setEnrichCvr] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<DataImportSummary | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  const alive = useRef(true);
  const selection = useRef(0);
  const sending = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; selection.current++; }; }, []);

  const outcome = useMutationOutcome(onImported);
  const guard = useDiscardGuard(!done && Boolean(content), onDismiss);
  const { onClose } = guard;

  async function handleFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    const version = ++selection.current;
    setContent(null);
    setFileName(null);
    setError(null);
    if (!file) {
      setFileName(null);
      setContent(null);
      return;
    }
    try {
      const text = await file.text();
      if (!alive.current || version !== selection.current) return;
      setFileName(file.name);
      setContent(text);
    } catch {
      if (!alive.current || version !== selection.current) return;
      setError("Filen kunne ikke læses.");
      setFileName(null);
      setContent(null);
    }
  }

  async function handleImport() {
    if (sending.current || outcome.isBlocked()) return;
    if (!content || !fileName) {
      setError("Vælg en fil først.");
      return;
    }
    sending.current = true;
    setBusy(true);
    setError(null);
    setLocked(null);
    try {
      const summary = await api.importData(slug, {
        fileName,
        content,
        enrichCvr,
      }).catch(outcome.reject);
      if (!alive.current) return;
      setDone(summary);
      onImported();
    } catch (err) {
      if (!alive.current) return;
      const e = err as MaybeApiError;
      const message = e?.message ?? "Importen kunne ikke gennemføres.";
      // A 409 conflict from the backup lock is shown kindly, not as an error.
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
    } finally {
      sending.current = false;
      if (alive.current) setBusy(false);
    }
  }

  return (
    <Dialog title="Importér fil" onClose={onClose} busy={busy} initialFocusRef={closeRef} xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}>
    {outcome.feedback}
      {guard.confirmation}



        {done ? (
          <ImportReceipt done={done} closeRef={closeRef} onClose={onClose} />
        ) : (
          <>
            <div {...stylex.props(cockpitStyles.modalBody)}>
              <p {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalBodyP,
							)}>
                Vælg en eksportfil fra dit tidligere bogføringssystem —
                Rentemester genkender selv formatet. Understøttet nu: Dinero
                «Kontakter» (Kontakter.csv) med kunder og leverandører.
              </p>
            </div>

            {locked && <LockBanner message={locked} />}
            {error && <Banner kind="error">{error}</Banner>}

            <label {...stylex.props(cockpitStyles.modalField)}>
              Fil
              <Input
                type="file"
                accept=".csv,text/csv"
                onChange={handleFile}
                disabled={outcome.blocked || (busy)}
              />
            </label>
            {fileName && (
              <p {...stylex.props(...[cockpitStyles.muted, viewStyles.site0])} >
                Valgt: {fileName}
              </p>
            )}

            <label {...stylex.props(cockpitStyles.modalField, cockpitStyles.modalCheckbox)}>
              <Input
                type="checkbox"
                checked={enrichCvr}
                onChange={(e) => setEnrichCvr(e.target.checked)}
                disabled={outcome.blocked || (busy)}
              />
              Berig danske virksomheder med adresse m.m. fra CVR-registeret
            </label>

            <div {...stylex.props(cockpitStyles.modalActions)}>
              <Button variant="secondary"
                type="button"

                onClick={onClose}
                disabled={busy} xstyle={[cockpitStyles.buttonComposition]}
              >
                Annullér
              </Button>
              <Button requiredPermission="company.ledger.post"
                type="button"

                onClick={handleImport}
                disabled={outcome.blocked || (busy || !content)}
              >
                {busy ? "Importerer…" : "Importér"}
              </Button>
            </div>
          </>
        )}

    </Dialog>
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
  // One source contact can attempt enrichment for both customer and vendor.
  // Cached success and failures can coexist in the same import.
  const cvrCredsMissing = done.errors.some((e) => e.includes("CVR_USERNAME"));
  // Errors that are NOT CVR enrichment — skipped rows, rows that could not be
  // created. The summary counts hide these, so they must be surfaced or the
  // owner sees "X oprettet" with no hint that rows were dropped.
  const cvrErrors = done.errors.filter((e) => e.includes("CVR-berigelse"));
  const otherErrors = done.errors.filter((e) => !e.includes("CVR-berigelse"));

  return (
    <>
      <div {...stylex.props(cockpitStyles.modalBody)}>
        <p {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.modalBodyP,
					)}>
          {done.detected
            ? `Genkendt som ${done.detected.label}.`
            : "Filen blev importeret."}
        </p>
        <p {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.modalBodyP,
					)}>
          {created} {created === 1 ? "kontakt" : "kontakter"} oprettet (
          {s.customersCreated}{" "}
          {s.customersCreated === 1 ? "kunde" : "kunder"},{" "}
          {s.vendorsCreated}{" "}
          {s.vendorsCreated === 1 ? "leverandør" : "leverandører"})
          {s.skipped > 0 ? ` · ${s.skipped} fandtes allerede` : ""}
          {s.enriched > 0 ? ` · ${s.enriched} beriget med CVR-data` : ""}.
        </p>
      </div>

      {cvrCredsMissing && (
        <Banner kind="warning">
          Adgang til CVR-registeret er ikke konfigureret for de mislykkede opslag.
          Bed administratoren konfigurere CVR_USERNAME og CVR_PASSWORD.
          Eventuel berigelse med gemte CVR-data fremgår af kvitteringen.
        </Banner>
      )}
      {s.enrichmentFailures > 0 && (
        <Banner kind="warning">
          CVR-berigelse mislykkedes i {s.enrichmentFailures}{" "}
          forsøg for kunde-/leverandørroller.
          Samme kontakt kan tælle to gange. Importen bruger filens oplysninger;
          eventuelle fejl ved oprettelse vises særskilt.
        </Banner>
      )}
      {cvrErrors.length > 0 && <details><summary>Bemærkninger til CVR-berigelse</summary><ul>{cvrErrors.map((message,index)=><li key={index}>{message}</li>)}</ul></details>}
      {otherErrors.length > 0 && (
        <Banner kind="warning">
          {otherErrors.length === 1
            ? otherErrors[0]
            : `${otherErrors.length} importbemærkninger: ` +
              otherErrors.slice(0, 3).join("; ") +
              (otherErrors.length > 3 ? " …" : "")}
        </Banner>
      )}

      <div {...stylex.props(cockpitStyles.modalActions)}>
        <Button
          type="button"

          ref={closeRef}
          onClick={onClose} xstyle={[cockpitStyles.buttonComposition]}
        >
          Luk
        </Button>
      </div>
    </>
  );
}

const viewStyles = stylex.create({
site0: { margin: 0 }
});
