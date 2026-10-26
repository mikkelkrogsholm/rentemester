import type { Database } from "bun:sqlite";

type SelectedBankTransaction = {
  id: number; transaction_date: string; amount: number; currency: string | null;
  amount_dkk: number | null; fx_rate_to_dkk: number | null; text: string; reference: string | null;
};

/** A reference identifies evidence only when it resolves to exactly one row. */
export function selectBankTransaction(db: Database, input: { bankTransactionId?: number; bankTransactionReference?: string }):
  { bank: SelectedBankTransaction; error?: never } | { error: string; bank?: never } {
  if (input.bankTransactionId === undefined && !input.bankTransactionReference) return { error: "bankTransactionId or bankTransactionReference is required" };
  const columns = "id, transaction_date, amount, currency, amount_dkk, fx_rate_to_dkk, text, reference";
  const rows = input.bankTransactionId !== undefined
    ? db.query(`SELECT ${columns} FROM bank_transactions WHERE id = ?`).all(input.bankTransactionId)
    : db.query(`SELECT ${columns} FROM bank_transactions WHERE reference = ? ORDER BY id DESC LIMIT 2`).all(input.bankTransactionReference!);
  if (rows.length > 1) return { error: `bank transaction reference ${input.bankTransactionReference} is ambiguous; provide bankTransactionId` };
  const bank = rows[0] as SelectedBankTransaction | undefined;
  if (!bank) return { error: input.bankTransactionId !== undefined ? `bank transaction ${input.bankTransactionId} does not exist` : `no bank transaction found with reference ${input.bankTransactionReference}` };
  if (input.bankTransactionId !== undefined && input.bankTransactionReference !== undefined && bank.reference !== input.bankTransactionReference) {
    return { error: "bankTransactionId and bankTransactionReference must identify the same bank transaction" };
  }
  return { bank };
}
