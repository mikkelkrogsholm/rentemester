import type {BankMatchKind} from "../lib/types/bank-matches";
import {bankMatchesApi} from "../lib/api/bank-matches";
import {formatKroner} from "../lib/format";
import {useAsync} from "../lib/useAsync";
import type {BankReconcileTransaction} from "./BankReconcileModal";
import {ErrorState, Loading} from "./Feedback";
import {Button, Dialog} from "./ui";

const KIND_LABELS: Record<BankMatchKind, string> = {
  issued_invoice: "Kundefaktura",
  purchase_sale: "Købsbilag",
  credit_note_refund: "Tilbagebetaling af kreditnota",
  supplier_credit_refund: "Mulig leverandørrefusion",
};

export function BankMatchSuggestionsModal({slug, transaction, onClose}: {
  slug: string;
  transaction: BankReconcileTransaction;
  onClose: () => void;
}) {
  const state = useAsync(signal => bankMatchesApi.bankSuggestMatches(slug, transaction.id, undefined, {signal}), [slug, transaction.id]);
  const row = state.data?.rows.find(row => row.bankTransactionId === transaction.id);
  return <Dialog title="Matchforslag til bankpost" onClose={onClose}>
    <p>Bankpost {transaction.id} · {transaction.date} · {transaction.text}{row && <> · {formatKroner(row.amount, row.currency)}</>}</p>
    <p>Forslagene er til gennemgang og ændrer ingen bogføring eller afstemning. Matchscore er en rangering, ikke en sandsynlighed; den kan være større end 1.</p>
    {state.loading ? <Loading label="Henter matchforslag…"/> : state.error ? <ErrorState message={state.error} onRetry={state.reload}/> : !state.data?.ok ? <ErrorState message={state.data?.errors.join(" ") || "Matchforslag kunne ikke hentes."} onRetry={state.reload}/> : !row ? <ErrorState message="Serveren returnerede ikke den valgte bankpost." onRetry={state.reload}/> : <>
      {row.unsafeMatchReason && <p role="status">{row.unsafeMatchReason}</p>}
      {!row.suggestions.length && <p>Ingen sikre matchforslag til denne bankpost. Gennemgå bilag og bankoplysninger før bogføring.</p>}
      <ul>{row.suggestions.map((suggestion, index) => <li key={`${suggestion.kind}:${suggestion.documentId}:${index}`}>
        <h3>{KIND_LABELS[suggestion.kind]}</h3>
        <p>Bilag {suggestion.documentId}{suggestion.invoiceNo ? ` · ${suggestion.invoiceNo}` : ""}{suggestion.customerName || suggestion.supplierName ? ` · ${suggestion.customerName || suggestion.supplierName}` : ""}</p>
        <p>Matchscore: {suggestion.confidence}</p>
        <ul>{suggestion.reasons.map((reason, i) => <li key={i}>{reason}</li>)}</ul>
        <a href={`/companies/${encodeURIComponent(slug)}/bilag?documentId=${suggestion.documentId}`}>Åbn bilag</a>
      </li>)}</ul>
    </>}
    <p>Luk forslagene, og vælg Bogfør på bankposten for at gennemgå et køb eller en kundebetaling. Tilbagebetaling til en kunde håndteres under Fakturaer efter gennemgang af kreditnotaen.</p>
    <p>Leverandørforslag er kun et spor til gennemgang af det oprindelige køb. De bekræfter ikke en kreditnota eller et tilgodehavende og kan ikke bogføres her.</p>
    <div className="modal-actions"><Button variant="secondary" onClick={onClose}>Luk</Button></div>
  </Dialog>;
}
