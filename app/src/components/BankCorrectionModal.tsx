import { useState } from "react";
import { Link } from "react-router-dom";
import { api } from "../lib/api";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Button, Dialog, Input } from "./ui";

type Plan =
  | {
      ok: true;
      plan: {
        reconciliationId: string;
        planHash: string;
        currentJournalEntryNo: string;
        replacementJournalEntryNo: string;
        bankAccountNo: string;
        bankAmountDkk: number;
        reversalJournalEntryNo?: string;
        reversalJournalHash?: string;
        currentJournalHash?: string;
        replacementJournalHash?: string;
        sourceHash?: string;
        effect?: string;
      };
    }
  | { ok: false; errors: string[] };

/** A deliberate two-step correction: inspect the hash-bound plan before apply. */
export function BankCorrectionModal({
  slug,
  transaction,
  onApplied,
  onClose: onDismiss,
  year,
}: {
  slug: string;
  transaction: { id: number; text: string; journalEntryNo?: string | null };
  onApplied: () => void;
  onClose: () => void;
  year?: string;
}) {
  const [replacementJournalEntryId, setReplacementJournalEntryId] = useState("");
  const [plan, setPlan] = useState<Plan | null>(null);
  const [reason, setReason] = useState("");
  const [idempotencyKey, setIdempotencyKey] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const outcome = useMutationOutcome(onApplied);
  const guard = useDiscardGuard(
    Boolean(replacementJournalEntryId || reason || idempotencyKey || confirmed),
    onDismiss,
  );
  const { onClose } = guard;
  const replacementId = Number(replacementJournalEntryId);

  async function inspect() {
    if (!Number.isInteger(replacementId) || replacementId <= 0) {
      setError("Angiv et positivt journal-id til erstatningen.");
      return;
    }
    setBusy(true);
    setError(null);
    setPlan(null);
    try {
      setPlan((await api.correctionPlan(slug, transaction.id, replacementId)) as Plan);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message.slice(0, 500) : "Planen kunne ikke læses.");
    } finally {
      setBusy(false);
    }
  }
  async function apply() {
    if (outcome.isBlocked()) return;
    if (!plan || !plan.ok || !confirmed || !reason.trim() || !idempotencyKey.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await api
        .applyCorrection(slug, {
          bankTransactionId: transaction.id,
          replacementJournalEntryId: replacementId,
          expectedReconciliationId: plan.plan.reconciliationId,
          planHash: plan.plan.planHash,
          reason: reason.trim(),
          idempotencyKey: idempotencyKey.trim(),
        })
        .catch(outcome.reject);
      onApplied();
      guard.dismiss();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message.slice(0, 500) : "Korrektionen blev afvist.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog title="Ret afstemt bankpost" onClose={onClose} busy={busy}>
      {outcome.feedback}
      {guard.confirmation}

      <p className="muted">
        Bankpost #{transaction.id}: {transaction.text}. Der tilføjes et nyt korrektionsbevis, som erstatter den aktuelle tilknytning. Historikken bevares.
      </p>
      <p>
        Tilbagefør først den eksisterende journal med en begrundelse og bogfør en selvstændig
        erstatning. Denne korrektion opretter kun et nyt afstemningsbevis; den tilbagefører eller
        bogfører ikke noget.
      </p>
      <Link to={`/companies/${slug}/posteringer${year ? `?year=${encodeURIComponent(year)}` : ""}`}>
        Åbn journal og forbered tilbageførsel/erstatning
      </Link>
      <label>
        Erstatningsjournal-id
        <Input
          aria-label="Erstatningsjournal-id"
          inputMode="numeric"
          value={replacementJournalEntryId}
          onChange={(event) => setReplacementJournalEntryId(event.target.value)}
          disabled={outcome.blocked || busy || !!plan}
        />
      </label>
      {!plan && (
        <div className="row-actions">
          <Button type="button" className="btn" onClick={inspect} disabled={busy}>
            Kontrollér plan
          </Button>
          <Button variant="secondary" type="button" className="btn secondary" onClick={onClose}>
            Annullér
          </Button>
        </div>
      )}
      {plan && !plan.ok && (
        <p className="error" role="alert">
          {plan.errors.join("; ").slice(0, 500)}
        </p>
      )}
      {plan?.ok && (
        <>
          <div className="card">
            <p>
              <strong>Eksisterende:</strong> {plan.plan.reconciliationId} ·{" "}
              {plan.plan.currentJournalEntryNo}
            </p>
            <p>
              <strong>Erstatning:</strong> {plan.plan.replacementJournalEntryNo} · konto{" "}
              {plan.plan.bankAccountNo} · {plan.plan.bankAmountDkk} DKK
            </p>
            {plan.plan.reversalJournalEntryNo && (
              <p>
                <strong>Kontrolleret tilbageførsel:</strong> {plan.plan.reversalJournalEntryNo}
              </p>
            )}
            {plan.plan.sourceHash && <p className="muted">Bankkilde: {plan.plan.sourceHash}</p>}
            {plan.plan.reversalJournalHash && (
              <p className="muted">Tilbageførslens hash: {plan.plan.reversalJournalHash}</p>
            )}
            {plan.plan.replacementJournalHash && (
              <p className="muted">Erstatningens hash: {plan.plan.replacementJournalHash}</p>
            )}
            <p className="muted">Plan-hash: {plan.plan.planHash}</p>
          </div>
          <label>
            Begrundelse
            <Input
              disabled={outcome.blocked}
              aria-label="Begrundelse"
              value={reason}
              onChange={(event) => setReason(event.target.value)}
              maxLength={1000}
            />
          </label>
          <p className="muted">
            Brug en særskilt nøgle til denne gennemgåede korrektion. Samme nøgle og uændrede data
            kan genfinde et kendt resultat; et usikkert svar må ikke automatisk gensendes.
          </p>
          <label>
            Idempotensnøgle
            <Input
              disabled={outcome.blocked}
              aria-label="Idempotensnøgle"
              value={idempotencyKey}
              onChange={(event) => setIdempotencyKey(event.target.value)}
              maxLength={128}
            />
          </label>
          <label>
            <Input
              disabled={outcome.blocked}
              type="checkbox"
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
            />{" "}
            Jeg har gennemgået planen og vil anvende denne korrektion.
          </label>
          <div className="row-actions">
            <Button
              requiredPermission="company.ledger.post"
              type="button"
              className="btn"
              onClick={apply}
              disabled={
                outcome.blocked || busy || !confirmed || !reason.trim() || !idempotencyKey.trim()
              }
            >
              Anvend korrektion
            </Button>
            <Button
              variant="secondary"
              type="button"
              disabled={busy || outcome.blocked}
              onClick={() => {
                setPlan(null);
                setConfirmed(false);
                setError(null);
              }}
            >
              Kontrollér en ny plan
            </Button>
            <Button
              variant="secondary"
              type="button"
              className="btn secondary"
              onClick={onClose}
              disabled={busy}
            >
              Annullér
            </Button>
          </div>
        </>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </Dialog>
  );
}
