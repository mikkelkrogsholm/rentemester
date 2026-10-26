import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { ErrorState, Loading } from "./Feedback";
import { DocumentBookExpenseModal } from "./DocumentBookExpenseModal";
import type { BankReconcileModalProps } from "./BankReconcileModal";
import { Button, Dialog, Select } from "./ui";

export function BankExpenseModal({ slug, transaction, onReconciled, onClose }: BankReconcileModalProps) {
  const documents = useAsync(signal => api.documents(slug, { signal }), [slug]);
  const [selected, setSelected] = useState("");
  const [reviewing, setReviewing] = useState(false);
  if (reviewing && selected) return <DocumentBookExpenseModal slug={slug} documentId={Number(selected)} initialBankTransactionId={transaction.id} onBooked={onReconciled} onClose={onClose} />;
  const available = documents.data?.documents.filter(document => !document.journalEntryId && ["purchase_sale", "cash_register_receipt"].includes(document.documentType)) ?? [];
  return <Dialog title="Vælg bilag til bankpost" onClose={onClose}>
    <p>{transaction.text} · {formatKroner(transaction.amount, transaction.currency)}</p>
    <p>Vælg det bilag, der dokumenterer betalingen. Kontrollér sammenhængen, kontoen og momsen før bogføring.</p>
    {documents.loading && <Loading />}
    {documents.error && <ErrorState message={documents.error} />}
    {documents.data && <><label htmlFor="bank-expense-document">Bilag</label><Select id="bank-expense-document" value={selected} onChange={event => setSelected(event.target.value)}><option value="">Vælg bilag…</option>{available.map(document => <option key={document.id} value={document.id}>{document.invoiceNo ?? document.documentNo ?? `Bilag ${document.id}`} · {document.supplierName ?? document.filename ?? "Bilag"} · {document.amountIncVat === null ? "Beløb mangler" : formatKroner(document.amountIncVat, document.currency)}</option>)}</Select>{!available.length && <p>Der er ingen ubogførte købsbilag. Indlæs bilaget under Bilag først.</p>}</>}
    <div {...stylex.props(cockpitStyles.rowActions)}><Button variant="secondary" onClick={onClose}>Luk</Button><Button disabled={!selected} onClick={() => setReviewing(true)}>Gennemgå bilag</Button></div>
  </Dialog>;
}
