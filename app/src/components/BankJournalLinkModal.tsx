import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import type { BankReconcileTransaction } from "./BankReconcileModal";
import { ErrorState, Loading } from "./Feedback";
import { Button, Dialog, Input, Select } from "./ui";

export function BankJournalLinkModal({
  slug,
  transaction,
  onApplied,
  onClose,
}: {
  slug: string;
  transaction: BankReconcileTransaction;
  onApplied: () => void;
  onClose: () => void;
}) {
  const state = useAsync(
    (signal) => api.bankReconciliationCandidates(slug, transaction.id, { signal }),
    [slug, transaction.id],
  );
  const [selected, setSelected] = useState("");
  const [note, setNote] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const outcome = useMutationOutcome(onApplied);
  const guard = useDiscardGuard(Boolean(selected || note), onClose);
  const plan = state.data?.plans.find((p) => String(p.journalEntryId) === selected);
  async function apply() {
    if (!plan || !confirmed || !note.trim() || busy || outcome.isBlocked()) return;
    setBusy(true);
    setError("");
    try {
      await api
        .linkBankJournal(slug, {
          bankTransactionId: transaction.id,
          journalEntryId: plan.journalEntryId,
          planHash: plan.planHash,
          note: note.trim(),
          confirm: true,
        })
        .catch(outcome.reject);
      onApplied();
      guard.dismiss();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Afstemningslinket blev ikke bekræftet.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog title="Afstem eksisterende postering" onClose={guard.onClose} busy={busy}>
      {outcome.feedback}
      {guard.confirmation}
      <p>
        {transaction.date} · {transaction.text} · {formatKroner(transaction.amount, "DKK")}
      </p>
      <p>
        Linket dokumenterer, at bankrækken og en allerede bogført bankbevægelse er samme hændelse.
        Der oprettes ingen postering, betaling eller ændring af den oprindelige journal.
      </p>
      {state.loading ? (
        <Loading label="Kontrollerer bankkilde og posteringer…" />
      ) : state.error ? (
        <ErrorState message={state.error} onRetry={state.reload} />
      ) : (
        <>
          <p {...stylex.props(cockpitStyles.muted)}>{state.data?.note}</p>
          {!state.data?.plans.length ? (
            <p>
              Ingen egnede, uafstemte posteringer med samme konto og beløb. Kontrollér kontomapping
              og allerede bogført beløb.
            </p>
          ) : (
            <label>
              Eksisterende postering
              <Select
                aria-label="Eksisterende postering"
                value={selected}
                disabled={busy || outcome.blocked}
                onChange={(e) => {
                  setSelected(e.target.value);
                  setConfirmed(false);
                }}
              >
                <option value="">Vælg postering efter gennemgang</option>
                {state.data.plans.map((p) => (
                  <option key={p.journalEntryId} value={p.journalEntryId}>
                    {p.journalEntryNo} · {p.journalDate} ·{" "}
                    {formatKroner(p.journalBankMovement, "DKK")}
                  </option>
                ))}
              </Select>
            </label>
          )}
          {plan && (
            <>
              <dl>
                <dt>Bankkonto</dt>
                <dd>{plan.bankAccountNo}</dd>
                <dt>Bankbeløb i DKK</dt>
                <dd>{formatKroner(plan.bankAmountDkk, "DKK")}</dd>
                <dt>Journalens bankbevægelse</dt>
                <dd>{formatKroner(plan.journalBankMovement, "DKK")}</dd>
                <dt>Datoer</dt>
                <dd>
                  Bank {plan.bankDate} · journal {plan.journalDate}
                </dd>
              </dl>
              <p {...stylex.props(cockpitStyles.muted)}>Bankkildens SHA-256: {plan.sourceHash}</p>
              <p {...stylex.props(cockpitStyles.muted)}>Journalens hash: {plan.journalHash}</p>
              <p {...stylex.props(cockpitStyles.muted)}>Review-hash: {plan.planHash}</p>
              {plan.document && (
                <a
                  href={`/api/companies/${encodeURIComponent(slug)}/documents/${plan.document.id}/file`}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Åbn journalens originalbilag
                </a>
              )}
              <label>
                Begrundelse
                <Input
                  aria-label="Begrundelse for afstemning"
                  value={note}
                  maxLength={1000}
                  disabled={busy || outcome.blocked}
                  onChange={(e) => setNote(e.target.value)}
                />
              </label>
              <label>
                <Input
                  type="checkbox"
                  checked={confirmed}
                  disabled={busy || outcome.blocked}
                  onChange={(e) => setConfirmed(e.target.checked)}
                />{" "}
                Jeg har gennemgået datoer, kilde og den eksisterende bankbevægelse.
              </label>
              <Button
                type="button"
                requiredPermission="company.ledger.post"
                disabled={busy || outcome.blocked || !confirmed || !note.trim()}
                onClick={apply}
              >
                Opret afstemningslink
              </Button>
            </>
          )}
        </>
      )}
      {error && <p role="alert">{error}</p>}
      <Button variant="secondary" type="button" disabled={busy} onClick={guard.onClose}>
        Luk
      </Button>
    </Dialog>
  );
}
