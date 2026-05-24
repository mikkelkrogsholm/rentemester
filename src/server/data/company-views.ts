// Per-company operational views for the cockpit (#320).
//
// Split out of `server/data.ts` by #320. The year-aware journal, bank, VAT,
// documents, invoices, contacts, obligations and cashflow views, plus the
// read-only company-settings view and the CVR sync. Every figure is computed
// by an existing core function or a shared data helper — no business logic is
// duplicated and nothing here mutates a ledger (CVR sync aside, which goes
// through the core `syncCompanyFromCvr`). Behaviour is unchanged from the
// pre-split `server/data.ts`. Money is kroner throughout.

import { existsSync } from "node:fs";
import { companyPaths } from "../../core/paths";
import { diffDaysSafe as daysBetween } from "../../core/dates";
import { openDb, migrate } from "../../core/db";
import {
  getCompanySettings,
  resolveCompanyPaymentDetails,
  syncCompanyFromCvr,
  type CompanyPaymentDetails,
  type CompanySettings,
  type SyncCompanyFromCvrResult,
} from "../../core/company";
import { fiscalYearForDate } from "../../core/fiscal-year";
import { buildInvoiceList } from "../../core/invoice-list";
import { listCustomers, listVendors } from "../../core/master-data";
import { listBankAccounts } from "../../core/bank";
import { resolveDocumentFile } from "../../core/documents";
import { renderIssuedInvoicePdf } from "../../core/invoice-pdf";
import {
  listRecurringInvoiceGenerations,
  listRecurringInvoiceTemplates,
} from "../../core/recurring-invoices";
import {
  vatPeriodWindowFor,
  vatPeriodsForYear,
  vatPeriodLabel,
  type EffectivePeriodState,
} from "../../core/periods";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../core/workspace";
import { ApiError } from "../errors";
import {
  buildCompanyFiscalYears,
  requireCompanyDbPath,
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
  todayIsoDate,
  MONTH_NAMES_DK,
} from "./shared";
import { bankBalanceAsOf, actualBankBalanceAsOf } from "./bank";
import { archiveYearRow } from "./archive";
import {
  selectVatPeriod,
  vatPositionForPeriod,
  vatPeriodEffectiveStatus,
  vatRubrikkerForPeriod,
  emptyVatRubrikker,
} from "./vat";

// --------------------------------------------------------------------------
// Company settings (read-only) + CVR sync
// --------------------------------------------------------------------------

/**
 * The full company settings row plus the company's own payment/bank details.
 *
 * `payment` is the primary `bank_accounts` row resolved via
 * `core/company.ts#resolveCompanyPaymentDetails` — the same source every issued
 * invoice's payment block reads from. It is null when no bank account is
 * configured yet, which is exactly when the Cockpit must let the owner add one
 * (#284): without it, an invoice goes out with no payment instructions.
 */
export type CompanySettingsView = CompanySettings & {
  payment: CompanyPaymentDetails | null;
};

/**
 * The full company settings row, including the CVR-register stamdata and the
 * payment/bank details. Read-only — backs `GET /api/companies/:slug/company` so
 * the cockpit can show the synced address/branche/status and the bank account.
 */
export function buildCompanySettings(
  workspaceRoot: string,
  slug: string,
): CompanySettingsView {
  const db = openDb(requireCompanyDbPath(workspaceRoot, slug));
  try {
    migrate(db);
    const settings = getCompanySettings(db);
    const payment = resolveCompanyPaymentDetails(db, settings.currency) ?? null;
    return { ...settings, payment };
  } finally {
    db.close();
  }
}

/**
 * Refresh a company's CVR-register stamdata. Backs
 * `POST /api/companies/:slug/sync-cvr`. The CVR lookup runs server-side so the
 * CVR credentials never reach the browser.
 */
export async function syncCompanyCvr(
  workspaceRoot: string,
  slug: string,
): Promise<SyncCompanyFromCvrResult> {
  const db = openDb(requireCompanyDbPath(workspaceRoot, slug));
  try {
    migrate(db);
    return await syncCompanyFromCvr(db);
  } finally {
    db.close();
  }
}

// --------------------------------------------------------------------------
// Per-company journal (Posteringer, year-aware) — cockpit-redesign it. 3
// --------------------------------------------------------------------------

export type JournalLine = {
  accountNo: string;
  accountName: string;
  debit: number;
  credit: number;
  text: string | null;
};

export type JournalEntry = {
  id: number;
  entryNo: string;
  date: string;
  text: string;
  /** Sum of the debit side — the entry total, kroner. */
  total: number;
  lines: JournalLine[];
  /**
   * #379 — the linked document's id (kvittering/faktura/kontoudtog) so the
   * cockpit can take the owner from an entry straight to its bilag, instead
   * of forcing them to guess from the text field. `null` when the entry has
   * no underlying document (manuel kassekladdepost).
   */
  documentId: number | null;
  /** The linked document's `document_no` for display. `null` when not linked. */
  documentNo: string | null;
};

export type CompanyJournal = ReturnType<typeof buildCompanyJournal>;

/**
 * Posteringer — every posted journal entry for the selected calendar fiscal
 * year, newest first, each carrying its debit/credit lines so the UI can drill
 * into an entry. The entry `total` is the summed debit side. Money is kroner.
 *
 * When `account` is given, only entries with at least one line on that account
 * are returned, and `accountFilter` names the account — this powers the
 * "klik konto → kontoens posteringer" drill-down from the statement views.
 */
export function buildCompanyJournal(
  workspaceRoot: string,
  slug: string,
  year: number | null,
  account: string | null = null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    if (ctx.isArchivedOnly) {
      // Archived year — group the archived Posteringer (#197) by their voucher
      // (Bilag) number into journal-entry-shaped rows. The export stores one
      // signed `amount` per line (the raw Beløb): a positive amount reads as a
      // debit, a negative one as a credit. Lines with no voucher are grouped
      // under a synthetic key so nothing is dropped.
      const archYear = parseInt(ctx.selectedLabel, 10);
      const header = archiveYearRow(ctx.db, archYear);
      const accountArg =
        account !== null && account.trim().length > 0 ? account.trim() : null;
      let entries: JournalEntry[] = [];
      let accountFilter: { accountNo: string; name: string } | null = null;
      if (header) {
        const postingRows = ctx.db
          .query(
            `SELECT line_no       AS lineNo,
                    account_no    AS accountNo,
                    account_name  AS accountName,
                    transaction_date AS date,
                    voucher       AS voucher,
                    text          AS text,
                    amount        AS amount
               FROM import_archive_postings
              WHERE archive_year_id = ?
              ORDER BY line_no ASC`,
          )
          .all(header.id) as Array<{
          lineNo: number;
          accountNo: string;
          accountName: string | null;
          date: string | null;
          voucher: string | null;
          text: string | null;
          amount: number;
        }>;

        if (accountArg !== null) {
          const sample = postingRows.find((r) => r.accountNo === accountArg);
          accountFilter = {
            accountNo: accountArg,
            name: sample?.accountName ?? accountArg,
          };
        }

        // Group by voucher; an absent voucher gets a stable synthetic key so
        // those lines still surface (one entry per orphan line).
        const groups = new Map<
          string,
          { voucher: string; date: string; lines: typeof postingRows }
        >();
        for (const r of postingRows) {
          const key = r.voucher && r.voucher.length > 0
            ? r.voucher
            : `linje-${r.lineNo}`;
          const existing = groups.get(key);
          if (existing) {
            existing.lines.push(r);
            if (r.date && (!existing.date || r.date < existing.date)) {
              existing.date = r.date;
            }
          } else {
            groups.set(key, {
              voucher: r.voucher && r.voucher.length > 0 ? r.voucher : key,
              date: r.date ?? `${archYear}-01-01`,
              lines: [r],
            });
          }
        }

        let all: JournalEntry[] = [...groups.values()].map((g, i) => {
          const lines: JournalLine[] = g.lines.map((r) => ({
            accountNo: r.accountNo,
            accountName: r.accountName ?? "",
            debit: r.amount > 0 ? roundKroner(r.amount) : 0,
            credit: r.amount < 0 ? roundKroner(-r.amount) : 0,
            text: r.text,
          }));
          const total = roundKroner(
            lines.reduce((acc, l) => acc + l.debit, 0),
          );
          // The entry text is the first non-empty line text — the Dinero
          // export repeats the voucher description across its lines.
          const text =
            g.lines.find((r) => r.text && r.text.length > 0)?.text ?? "";
          return {
            id: i + 1,
            entryNo: g.voucher,
            date: g.date,
            text,
            total,
            lines,
            // #379 — arkiverede Posteringer har ingen bilag-linkage; det
            // arkiverede materiale lever uden for `documents`/`journal_entries`
            // og kan ikke resolves til en åbnbar fil.
            documentId: null,
            documentNo: null,
          };
        });

        // Newest first — the same ordering the live journal uses.
        all.sort((a, b) =>
          a.date !== b.date
            ? b.date.localeCompare(a.date)
            : b.entryNo.localeCompare(a.entryNo),
        );

        entries =
          accountArg === null
            ? all
            : all.filter((e) =>
                e.lines.some((l) => l.accountNo === accountArg),
              );
      }
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        archivedSource: header?.sourceSystem ?? null,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: `${ctx.selectedLabel}-01-01`,
        periodEnd: `${ctx.selectedLabel}-12-31`,
        entries,
        accountFilter,
      };
    }

    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;

    // Optional account drill-down: resolve the account's name (so the view can
    // title the filter) and the set of entry ids that touch it.
    let accountFilter: { accountNo: string; name: string } | null = null;
    let accountEntryIds: Set<number> | null = null;
    if (account !== null && account.trim().length > 0) {
      const accountNo = account.trim();
      const acc = ctx.db
        .query("SELECT account_no AS accountNo, name AS name FROM accounts WHERE account_no = ?")
        .get(accountNo) as { accountNo: string; name: string } | undefined;
      accountFilter = acc
        ? { accountNo: acc.accountNo, name: acc.name }
        : { accountNo, name: accountNo };
      const idRows = ctx.db
        .query(
          `SELECT DISTINCT jl.journal_entry_id AS id
             FROM journal_lines jl
             JOIN journal_entries je ON je.id = jl.journal_entry_id
             JOIN accounts a         ON a.id = jl.account_id
            WHERE je.status = 'posted'
              AND je.transaction_date >= ? AND je.transaction_date <= ?
              AND a.account_no = ?`,
        )
        .all(yearStart, yearEnd, accountNo) as Array<{ id: number }>;
      accountEntryIds = new Set(idRows.map((r) => r.id));
    }

    // #379 — pull the linked bilag (document) onto each entry via the direct
    // `journal_entries.document_id` foreign key, with `import_document_links`
    // as a fall-back for legacy posts where the documents/journal-entries were
    // wired only through the import-link table. The LEFT JOINs keep entries
    // without a document (manuel kassekladde) on the result, with `documentId`
    // and `documentNo` returned as null.
    const entryRows = ctx.db
      .query(
        `SELECT je.id          AS id,
                je.entry_no    AS entryNo,
                je.transaction_date AS date,
                je.text        AS text,
                COALESCE(je.document_id, idl.document_id) AS documentId,
                d.document_no  AS documentNo
           FROM journal_entries je
           LEFT JOIN import_document_links idl ON idl.journal_entry_id = je.id
           LEFT JOIN documents d
             ON d.id = COALESCE(je.document_id, idl.document_id)
          WHERE je.status = 'posted'
            AND je.transaction_date >= ? AND je.transaction_date <= ?
          ORDER BY je.transaction_date DESC, je.id DESC`,
      )
      .all(yearStart, yearEnd) as Array<{
      id: number;
      entryNo: string;
      date: string;
      text: string;
      documentId: number | null;
      documentNo: string | null;
    }>;

    const lineRows = ctx.db
      .query(
        `SELECT jl.journal_entry_id AS entryId,
                a.account_no        AS accountNo,
                a.name              AS accountName,
                jl.debit_amount     AS debit,
                jl.credit_amount    AS credit,
                jl.text             AS text
           FROM journal_lines jl
           JOIN journal_entries je ON je.id = jl.journal_entry_id
           JOIN accounts a         ON a.id = jl.account_id
          WHERE je.status = 'posted'
            AND je.transaction_date >= ? AND je.transaction_date <= ?
          ORDER BY jl.id ASC`,
      )
      .all(yearStart, yearEnd) as Array<{
      entryId: number;
      accountNo: string;
      accountName: string;
      debit: number;
      credit: number;
      text: string | null;
    }>;

    const linesByEntry = new Map<number, JournalLine[]>();
    for (const row of lineRows) {
      const list = linesByEntry.get(row.entryId) ?? [];
      list.push({
        accountNo: row.accountNo,
        accountName: row.accountName,
        debit: roundKroner(row.debit),
        credit: roundKroner(row.credit),
        text: row.text,
      });
      linesByEntry.set(row.entryId, list);
    }

    const filteredRows =
      accountEntryIds === null
        ? entryRows
        : entryRows.filter((e) => accountEntryIds!.has(e.id));

    const entries: JournalEntry[] = filteredRows.map((e) => {
      const lines = linesByEntry.get(e.id) ?? [];
      const total = roundKroner(
        lines.reduce((acc, l) => acc + l.debit, 0),
      );
      return {
        id: e.id,
        entryNo: e.entryNo,
        date: e.date,
        text: e.text,
        total,
        lines,
        documentId: e.documentId ?? null,
        documentNo: e.documentNo ?? null,
      };
    });

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      archivedSource: null as string | null,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: yearStart,
      periodEnd: yearEnd,
      entries,
      accountFilter,
    };
  } finally {
    ctx.db.close();
  }
}

// --------------------------------------------------------------------------
// Per-company bank transactions (Bank, year-aware) — cockpit-redesign it. 3
// --------------------------------------------------------------------------

export type BankTransactionRow = {
  id: number;
  date: string;
  text: string;
  amount: number;
  /** Running balance from the import, kroner; null when the export omits it. */
  runningBalance: number | null;
  /** "matched" when a posted journal entry references this row, else "unmatched". */
  reconciliationStatus: "matched" | "unmatched";
  /** The matched journal entry's number, when reconciled. */
  journalEntryNo: string | null;
};

export type CompanyBank = ReturnType<typeof buildCompanyBank>;

/**
 * Bank — the imported `bank_transactions` rows for the selected calendar
 * fiscal year, each with its reconciliation status (matched vs unmatched to a
 * posted journal entry), plus the registered bank account and its booked
 * ledger balance at the year end. Money is kroner.
 */
export function buildCompanyBank(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    const accounts = listBankAccounts(ctx.db).accounts.map((a) => ({
      id: a.id,
      name: a.name,
      bankName: a.bankName,
      accountNo: a.accountNo,
      ledgerAccountNo: a.ledgerAccountNo,
    }));
    // Archived years (#197) keep their LEDGER in the read-only archive — but a
    // bank statement is live, append-only data that legitimately spans both
    // archived and live years (a Dinero migration archives 2023-2025 while the
    // owner's bank CSV covers 2024-2026). The imported `bank_transactions` rows
    // are therefore shown for every selected year; only the `archived` flag
    // tells the cockpit to present them without the live-ledger reconciliation
    // and booked-balance comparison it cannot meaningfully do for those years.
    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;

    // Bank rows for the year, oldest-first so the running balance reads
    // naturally down the table. The LEFT JOIN to a posted journal entry on
    // `source_bank_transaction_id` is the reconciliation status — the same
    // join `core/reconciliation.listBankTransactions` uses.
    const rows = ctx.db
      .query(
        `SELECT bt.id            AS id,
                bt.transaction_date AS date,
                bt.text          AS text,
                bt.amount        AS amount,
                bt.balance_after AS runningBalance,
                je.entry_no      AS journalEntryNo
           FROM bank_transactions bt
           LEFT JOIN journal_entries je
             ON je.source_bank_transaction_id = bt.id
            AND je.status = 'posted'
          WHERE bt.transaction_date >= ? AND bt.transaction_date <= ?
          ORDER BY bt.transaction_date ASC, bt.id ASC`,
      )
      .all(yearStart, yearEnd) as Array<{
      id: number;
      date: string;
      text: string;
      amount: number;
      runningBalance: number | null;
      journalEntryNo: string | null;
    }>;
    const transactions: BankTransactionRow[] = rows.map((r) => ({
      id: r.id,
      date: r.date,
      text: r.text,
      amount: roundKroner(r.amount),
      runningBalance:
        r.runningBalance === null || r.runningBalance === undefined
          ? null
          : roundKroner(r.runningBalance),
      reconciliationStatus: r.journalEntryNo ? "matched" : "unmatched",
      journalEntryNo: r.journalEntryNo,
    }));
    const matchedCount = transactions.filter(
      (t) => t.reconciliationStatus === "matched",
    ).length;

    // The booked ledger balance vs the actual statement balance (the latest
    // imported `balance_after`). Their gap is the headline of a bank page —
    // money the owner has on paper but not in the account, or vice versa.
    const bookedBalance = bankBalanceAsOf(ctx.db, yearEnd);
    const actualBalance = actualBankBalanceAsOf(ctx.db, yearEnd);
    const difference =
      actualBalance === null ? null : roundKroner(bookedBalance - actualBalance);

    // #305: `actualBalance === null` has two very different causes — no
    // statement imported at all, or a statement WAS imported but its CSV had
    // no balance column. The cockpit must not say "intet kontoudtog
    // importeret" for the second case: an owner who just imported a CSV would
    // think the import silently failed. `bankStatementStatus` distinguishes
    // them so the UI can say "banksaldo ukendt — kontoudtoget havde ingen
    // saldo-kolonne" instead.
    const hasAnyTransactions =
      transactions.length > 0 ||
      (ctx.db
        .query("SELECT 1 FROM bank_transactions LIMIT 1")
        .get() as unknown) !== null;
    const bankStatementStatus: "known" | "no-balance-column" | "none" =
      actualBalance !== null
        ? "known"
        : hasAnyTransactions
          ? "no-balance-column"
          : "none";

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: ctx.isArchivedOnly,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: yearStart,
      periodEnd: yearEnd,
      accounts,
      bookedBalance,
      actualBalance,
      difference,
      bankStatementStatus,
      transactions,
      matchedCount,
      unmatchedCount: transactions.length - matchedCount,
    };
  } finally {
    ctx.db.close();
  }
}

// --------------------------------------------------------------------------
// Per-company VAT return (Moms, year-aware) — cockpit-redesign it. 3
// --------------------------------------------------------------------------

export type CompanyVat = ReturnType<typeof buildCompanyVat>;

/**
 * Moms — the VAT return for the selected calendar fiscal year. The VAT period
 * follows the company's own settlement cadence (`vatPeriodType` — month /
 * quarter / half-year, #299); the period that is due now is surfaced, the same
 * selection `buildCompanyOverview` uses. The figures come from the booked VAT
 * accounts via `vatPositionForPeriod`. Money is kroner.
 *
 * #303: `periodStatus` reports the period's effective lifecycle state. A
 * momsangivelse may only be FILED for a `closed`/`reported` period — for an
 * `open` period the figures are provisional, and the cockpit must say so
 * rather than claim they match the terminal `vat momsangivelse` (which refuses
 * an open period). `momsangivelseReady` is the single flag the SPA keys off.
 */
export function buildCompanyVat(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    if (ctx.isArchivedOnly) {
      const archYear = parseInt(ctx.selectedLabel, 10);
      const archWindow = vatPeriodWindowFor(
        `${archYear}-01-01`,
        ctx.company.vatPeriodType,
      );
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: archWindow.start,
        periodEnd: archWindow.end,
        periodLabel: vatPeriodLabel(archWindow),
        outputVat: 0,
        outputVatAdjustment: 0,
        inputVat: 0,
        payable: 0,
        deadline: archWindow.filingDeadline,
        daysRemaining: daysBetween(todayIsoDate(), archWindow.filingDeadline),
        // An archived year carries no live period to close — treat as open so
        // no provisional figures are ever claimed filing-ready.
        periodStatus: "open" as EffectivePeriodState,
        momsangivelseReady: false,
        rubrikker: emptyVatRubrikker(),
      };
    }

    const yearNum = parseInt(ctx.selectedLabel, 10);
    // Surface the VAT period (month / quarter / half-year, per the company's
    // `vatPeriodType`) that is due now — the same selection the static
    // dashboard and the Overblik view use, so the period type never depends on
    // which screen the owner looks at (#299).
    const vatSelection = selectVatPeriod(
      ctx.db,
      yearNum,
      ctx.company.vatPeriodType,
    );
    const vat = vatSelection.position;

    // The statutory filing/payment deadline for the surfaced period, plus a
    // signed countdown from today — negative once the deadline has passed.
    const deadline = vatSelection.deadline;

    // #303: a momsangivelse is only filing-ready for a closed/reported period.
    // For an open period the cockpit shows the figures as PROVISIONAL.
    const periodStatus = vatPeriodEffectiveStatus(
      ctx.db,
      vat.periodStart,
      vat.periodEnd,
    );
    const momsangivelseReady =
      periodStatus === "closed" || periodStatus === "reported";

    // The full SKAT TastSelv rubrics — the same numbers the CLI's
    // `vat momsangivelse` reports — so an owner can file straight from here.
    const rubrikker = vatRubrikkerForPeriod(
      ctx.db,
      vat.periodStart,
      vat.periodEnd,
    );

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: vat.periodStart,
      periodEnd: vat.periodEnd,
      periodLabel: vatSelection.label,
      outputVat: vat.outputVat,
      outputVatAdjustment: vat.outputVatAdjustment,
      inputVat: vat.inputVat,
      payable: vat.payable,
      deadline,
      daysRemaining: daysBetween(todayIsoDate(), deadline),
      periodStatus,
      momsangivelseReady,
      rubrikker,
    };
  } finally {
    ctx.db.close();
  }
}

// --------------------------------------------------------------------------
// Per-company documents (Bilag) — cockpit-redesign it. 3
// --------------------------------------------------------------------------

export type DocumentRow = {
  id: number;
  documentNo: string | null;
  source: string;
  filename: string | null;
  documentType: string;
  supplierName: string | null;
  invoiceNo: string | null;
  invoiceDate: string | null;
  amountIncVat: number | null;
  currency: string;
  status: string;
  /** The voucher reference the document was matched on, when linked. */
  voucherRef: string | null;
  /** The linked journal entry's number, when one exists. */
  journalEntryNo: string | null;
  /** The linked journal entry's id, for drill-through. */
  journalEntryId: number | null;
  /** The linked journal entry's posting text — what the voucher is for. */
  journalEntryText: string | null;
  /** The linked journal entry's total (summed debit side), kroner. */
  journalEntryTotal: number | null;
  /** True when the document has a stored file the cockpit can open. */
  hasFile: boolean;
};

export type CompanyDocuments = ReturnType<typeof buildCompanyDocuments>;

/**
 * Bilag — the ingested documents/receipts in the company's `documents` table,
 * each carrying the voucher and posted journal entry it is linked to through
 * `import_document_links` (#196) where one exists. Newest upload first.
 */
export function buildCompanyDocuments(workspaceRoot: string, slug: string) {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);
    // A bilag is "bogført" when either an import_document_links row binds it
    // to a journal entry (the legacy archive-import flow) OR a journal entry
    // directly references it via journal_entries.document_id (the native
    // bookkeeping flow used by `expense book`, `invoice post` and the
    // Bogfør-bilag modal in #407). Without the COALESCE the document keeps
    // looking "Ikke bogført" in the cockpit even after the entry posts, which
    // is the loop the modal is closing.
    const rows = db
      .query(
        `SELECT d.id              AS id,
                d.document_no     AS documentNo,
                d.source          AS source,
                d.original_filename AS filename,
                d.document_type   AS documentType,
                d.supplier_name   AS supplierName,
                d.invoice_no      AS invoiceNo,
                d.invoice_date    AS invoiceDate,
                d.amount_inc_vat  AS amountIncVat,
                d.currency        AS currency,
                d.status          AS status,
                d.stored_path     AS storedPath,
                idl.voucher_ref   AS voucherRef,
                COALESCE(je_link.id, je_direct.id)             AS journalEntryId,
                COALESCE(je_link.entry_no, je_direct.entry_no) AS journalEntryNo,
                COALESCE(je_link.text, je_direct.text)         AS journalEntryText,
                COALESCE(
                  (SELECT COALESCE(SUM(debit_amount), 0)
                     FROM journal_lines
                    WHERE journal_entry_id = je_link.id),
                  (SELECT COALESCE(SUM(debit_amount), 0)
                     FROM journal_lines
                    WHERE journal_entry_id = je_direct.id)
                )                                              AS journalEntryTotal
           FROM documents d
           LEFT JOIN import_document_links idl ON idl.document_id = d.id
           LEFT JOIN journal_entries je_link   ON je_link.id = idl.journal_entry_id
           LEFT JOIN journal_entries je_direct ON je_direct.document_id = d.id
          ORDER BY d.upload_datetime DESC, d.id DESC`,
      )
      .all() as Array<{
      id: number;
      documentNo: string | null;
      source: string;
      filename: string | null;
      documentType: string;
      supplierName: string | null;
      invoiceNo: string | null;
      invoiceDate: string | null;
      amountIncVat: number | null;
      currency: string;
      status: string;
      storedPath: string | null;
      voucherRef: string | null;
      journalEntryId: number | null;
      journalEntryNo: string | null;
      journalEntryText: string | null;
      journalEntryTotal: number | null;
    }>;

    const documents: DocumentRow[] = rows.map((r) => ({
      id: r.id,
      documentNo: r.documentNo,
      source: r.source,
      filename: r.filename,
      documentType: r.documentType,
      supplierName: r.supplierName,
      invoiceNo: r.invoiceNo,
      invoiceDate: r.invoiceDate,
      amountIncVat:
        r.amountIncVat === null || r.amountIncVat === undefined
          ? null
          : roundKroner(r.amountIncVat),
      currency: r.currency,
      status: r.status,
      voucherRef: r.voucherRef,
      journalEntryNo: r.journalEntryNo,
      journalEntryId: r.journalEntryId,
      journalEntryText: r.journalEntryText,
      journalEntryTotal:
        r.journalEntryId === null || r.journalEntryTotal === null
          ? null
          : roundKroner(r.journalEntryTotal),
      hasFile: r.storedPath != null,
    }));
    const linkedCount = documents.filter(
      (d) => d.journalEntryNo !== null,
    ).length;

    return {
      slug: entry.slug,
      company: statementCompanyBlock(company),
      documents,
      linkedCount,
      unlinkedCount: documents.length - linkedCount,
    };
  } finally {
    db.close();
  }
}

/**
 * Resolves a single ingested document's stored file so the cockpit can serve
 * it back to a human. The same notFound shape the other document reads use is
 * thrown for an unknown company, a missing ledger, or a document without a
 * readable file.
 */
export function resolveCompanyDocumentFile(
  workspaceRoot: string,
  slug: string,
  documentId: number,
): { path: string; mimeType: string; filename: string } {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }

  const db = openDb(dbPath);
  try {
    migrate(db);
    const resolved = resolveDocumentFile(db, companyRoot, documentId);
    if (!resolved.ok) {
      throw ApiError.notFound(resolved.error);
    }
    return resolved.file;
  } finally {
    db.close();
  }
}

// --- documents / Bilag-bogføring (#407) -----------------------------------
//
// The Cockpit's Bilag table shows "Ikke bogført" rows without an action — the
// owner can see the queue but cannot post anything from the browser. The pair
//
//   GET  .../documents/:id/booking-options    (this builder)
//   POST .../documents/book-expense           (handleDocumentBookExpense)
//
// closes that loop. The GET hands the modal everything it needs (the document
// fields to prefill, the bookable expense accounts, the unmatched outgoing
// bank transactions to pair with) and the POST is a thin adapter over the
// SAME `bookExpenseFromBank` core function the CLI's `expense book` command
// uses, so the Cockpit never reimplements bookkeeping.

/** One bookable expense account — the picker rows in the modal. */
export type ExpenseAccountOption = {
  accountNo: string;
  name: string;
  /** Hint for default VAT treatment; null when not configured. */
  defaultVatCode: string | null;
};

/** One unmatched outgoing bank transaction the owner can pair the bilag with. */
export type UnmatchedBankOption = {
  id: number;
  date: string;
  text: string;
  /** Original signed kroner amount — outgoing payments are negative. */
  amount: number;
  currency: string;
  reference: string | null;
};

/** The minimum bilag fields the modal prefills its form from. */
export type DocumentBookingOptionsDocument = {
  id: number;
  documentNo: string | null;
  documentType: string;
  invoiceNo: string | null;
  invoiceDate: string | null;
  supplierName: string | null;
  amountIncVat: number | null;
  vatAmount: number | null;
  currency: string;
};

export type DocumentBookingOptions = {
  document: DocumentBookingOptionsDocument;
  expenseAccounts: ExpenseAccountOption[];
  unmatchedOutgoingBank: UnmatchedBankOption[];
};

/**
 * Read-only view backing the Bogfør-bilag modal: the document being booked,
 * the bookable expense-account picker rows, and the unmatched outgoing bank
 * transactions the owner can pair it with. A 404 is thrown when the company
 * / ledger / document is missing; an already-linked document is allowed and
 * returns its current row (the core booker is the one that refuses a
 * double-post, with a clean Danish error).
 */
export function buildDocumentBookingOptions(
  workspaceRoot: string,
  slug: string,
  documentId: number,
): DocumentBookingOptions {
  if (!Number.isInteger(documentId) || documentId <= 0) {
    throw ApiError.badRequest("document id must be a positive integer");
  }
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }
  const db = openDb(dbPath);
  try {
    migrate(db);
    const doc = db
      .query(
        `SELECT id, document_no, document_type, invoice_no, invoice_date,
                supplier_name, amount_inc_vat, vat_amount, currency
           FROM documents
          WHERE id = ?`,
      )
      .get(documentId) as
      | {
          id: number;
          document_no: string | null;
          document_type: string;
          invoice_no: string | null;
          invoice_date: string | null;
          supplier_name: string | null;
          amount_inc_vat: number | null;
          vat_amount: number | null;
          currency: string;
        }
      | null;
    if (!doc) {
      throw ApiError.notFound(`document ${documentId} does not exist`);
    }
    const expenseAccounts = (db
      .query(
        `SELECT account_no, name, default_vat_code
           FROM accounts
          WHERE type = 'expense' AND active = 1
          ORDER BY account_no ASC`,
      )
      .all() as Array<{
      account_no: string;
      name: string;
      default_vat_code: string | null;
    }>).map((r) => ({
      accountNo: r.account_no,
      name: r.name,
      defaultVatCode: r.default_vat_code,
    }));
    // Outgoing = amount < 0. Unmatched = no journal entry already references
    // this row. Newest first, capped to a sensible picker length.
    const unmatchedOutgoingBank = (db
      .query(
        `SELECT bt.id          AS id,
                bt.transaction_date AS date,
                bt.text         AS text,
                bt.amount       AS amount,
                bt.currency     AS currency,
                bt.reference    AS reference
           FROM bank_transactions bt
          WHERE bt.amount < 0
            AND NOT EXISTS (
              SELECT 1 FROM journal_entries je
               WHERE je.source_bank_transaction_id = bt.id
            )
          ORDER BY bt.transaction_date DESC, bt.id DESC
          LIMIT 200`,
      )
      .all() as Array<{
      id: number;
      date: string;
      text: string;
      amount: number;
      currency: string;
      reference: string | null;
    }>).map((r) => ({
      id: r.id,
      date: r.date,
      text: r.text,
      amount: roundKroner(r.amount),
      currency: r.currency,
      reference: r.reference,
    }));
    return {
      document: {
        id: doc.id,
        documentNo: doc.document_no,
        documentType: doc.document_type,
        invoiceNo: doc.invoice_no,
        invoiceDate: doc.invoice_date,
        supplierName: doc.supplier_name,
        amountIncVat:
          doc.amount_inc_vat === null
            ? null
            : roundKroner(doc.amount_inc_vat),
        vatAmount:
          doc.vat_amount === null ? null : roundKroner(doc.vat_amount),
        currency: doc.currency,
      },
      expenseAccounts,
      unmatchedOutgoingBank,
    };
  } finally {
    db.close();
  }
}

/**
 * Resolves an issued-invoice document into the on-disk PDF the cockpit can
 * serve to the owner. The same `renderIssuedInvoicePdf` core that the CLI's
 * `invoice render` command uses is called — re-rendering is idempotent: when
 * the payload has not changed since issuance, the existing PDF row is returned
 * unchanged; if it has, the row is updated in place. The thrown shapes match
 * `resolveCompanyDocumentFile` so the route handler can rely on the same
 * not-found mapping. (#378)
 */
export function resolveCompanyIssuedInvoicePdf(
  workspaceRoot: string,
  slug: string,
  invoiceDocumentId: number,
): { path: string; mimeType: string; filename: string } {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }

  const db = openDb(dbPath);
  try {
    migrate(db);
    const result = renderIssuedInvoicePdf(db, companyRoot, {
      invoiceDocumentId,
    });
    if (!result.ok || !result.storedPath || !result.invoiceNumber) {
      const reason = result.errors[0] ?? "issued invoice PDF could not be rendered";
      throw ApiError.notFound(reason);
    }
    return {
      path: result.storedPath,
      mimeType: "application/pdf",
      filename: `${result.invoiceNumber}.pdf`,
    };
  } finally {
    db.close();
  }
}

// --------------------------------------------------------------------------
// Per-company issued invoices (Fakturaer, year-aware) — cockpit-redesign it. 5
// --------------------------------------------------------------------------

/**
 * Cockpit-facing PEPPOL/e-faktura status (#428) — verbatim copy of the
 * core `InvoicePeppolStatus`, exposed so the Cockpit can flag
 * "Sendt som e-faktura" on a row and gate the send-action.
 */
export type CompanyInvoicePeppolStatus = {
  status: "prepared" | "acknowledged";
  submissionReference: string;
  transmissionId: string | null;
  acknowledgedAt: string | null;
};

/** One issued invoice — the fields the Fakturaer view renders. */
export type CompanyInvoiceRow = {
  documentId: number;
  invoiceNo: string;
  invoiceDate: string | null;
  customerName: string | null;
  /**
   * Customer's e-mail when set on the kontaktkort (#429). Surfaced so the
   * cockpit can render a "Send på mail" action only on rows where there is
   * a stored e-mail to prefill, and the dialog has no second round-trip.
   */
  customerEmail: string | null;
  /**
   * Buyer's EAN-number normalised to 13 digits, or `null`. #428: the
   * Cockpit row offers "Send som e-faktura" only when this is set AND the
   * buyer is marked as a public recipient.
   */
  buyerEanNumber: string | null;
  /** True when the invoice was issued to a public-recipient buyer. */
  buyerPublicRecipient: boolean;
  /**
   * The most recent recorded PEPPOL submission/transmission status, or
   * `null` when the invoice has never been sent as an e-faktura.
   */
  peppolStatus: CompanyInvoicePeppolStatus | null;
  /**
   * ISO-8601 timestamp of the most recent `email_send_log` row for this
   * invoice (#429), or `null` when the invoice has never been emailed
   * from the cockpit. Read-only audit data — the cockpit uses it only to
   * surface "Sendt {dato}" beside the settlement status.
   */
  lastEmailedAt: string | null;
  /** Gross amount inc. VAT, kroner. */
  grossAmount: number;
  /** Still-outstanding balance on the invoice, kroner. */
  openBalance: number;
  currency: string;
  /** Settlement state, plus "overdue" for an open invoice past its due date. */
  status: "open" | "paid" | "credited" | "refunded" | "overpaid" | "written_off" | "overdue";
  effectiveDueDate: string | null;
  overdueDays: number;
};

export type CompanyInvoices = ReturnType<typeof buildCompanyInvoices>;

/**
 * Fakturaer — the company's issued (sales) invoices for the selected calendar
 * fiscal year. Each row carries its settlement status; an open invoice past
 * its due date is surfaced as "overdue". Every figure comes from
 * `core/invoice-list` (which derives status via `core/invoice-payments`) — no
 * business logic is duplicated. A company with no issued invoices returns an
 * empty list, which is a correct, expected state.
 */
export function buildCompanyInvoices(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    if (ctx.isArchivedOnly) {
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: `${ctx.selectedLabel}-01-01`,
        periodEnd: `${ctx.selectedLabel}-12-31`,
        invoices: [] as CompanyInvoiceRow[],
        totalGross: 0,
        totalOpen: 0,
        overdueCount: 0,
      };
    }

    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;

    const list = buildInvoiceList(ctx.db, { from: yearStart, to: yearEnd });
    // #429 — match the row's `customerName` against the kontakter so the
    // cockpit can prefill the "Send på mail" dialog with the stored e-mail.
    // Read once and lookup in-memory: a typical company has tens of customers,
    // and this view already runs a per-row payments query through the core.
    const customerEmailByName = new Map<string, string>();
    try {
      const customerRows = ctx.db
        .query(
          `SELECT name, email FROM customers
           WHERE archived = 0 AND email IS NOT NULL AND TRIM(email) <> ''`,
        )
        .all() as Array<{ name: string; email: string | null }>;
      for (const c of customerRows) {
        if (c.name && typeof c.email === "string" && c.email.trim() !== "") {
          customerEmailByName.set(c.name.trim(), c.email.trim());
        }
      }
    } catch {
      // Older fixture without a customers table — degrade to no prefill.
    }
    // #429 — look up the most recent email_send_log row per invoice so the
    // cockpit can surface "Sendt {dato}". The table is append-only; an
    // older db without the migration is treated as "never sent".
    const lastEmailedAtById = new Map<number, string>();
    try {
      const sendRows = ctx.db
        .query(
          `SELECT invoice_document_id, MAX(created_at) AS last_at
           FROM email_send_log
           WHERE kind = 'invoice'
           GROUP BY invoice_document_id`,
        )
        .all() as Array<{ invoice_document_id: number; last_at: string | null }>;
      for (const s of sendRows) {
        if (s.last_at) lastEmailedAtById.set(s.invoice_document_id, s.last_at);
      }
    } catch {
      // email_send_log not migrated — no rows to surface.
    }
    const invoices: CompanyInvoiceRow[] = list.rows.map((r) => ({
      documentId: r.documentId,
      invoiceNo: r.invoiceNumber,
      invoiceDate: r.invoiceDate,
      customerName: r.customerName,
      customerEmail: r.customerName
        ? customerEmailByName.get(r.customerName.trim()) ?? null
        : null,
      buyerEanNumber: r.buyerEanNumber,
      buyerPublicRecipient: r.buyerPublicRecipient,
      peppolStatus: r.peppolStatus,
      lastEmailedAt: lastEmailedAtById.get(r.documentId) ?? null,
      grossAmount: roundKroner(r.grossAmount),
      openBalance: roundKroner(r.openBalance),
      currency: r.currency,
      status: r.isOverdue ? "overdue" : r.status,
      effectiveDueDate: r.effectiveDueDate,
      overdueDays: r.overdueDays,
    }));
    // Newest invoice first — the most recent activity is what the user wants.
    invoices.sort((a, b) => {
      const dateA = a.invoiceDate ?? "";
      const dateB = b.invoiceDate ?? "";
      if (dateA !== dateB) return dateB.localeCompare(dateA);
      return b.invoiceNo.localeCompare(a.invoiceNo);
    });

    const totalGross = roundKroner(
      invoices.reduce((acc, r) => acc + r.grossAmount, 0),
    );
    const totalOpen = roundKroner(
      invoices.reduce((acc, r) => acc + r.openBalance, 0),
    );
    const overdueCount = invoices.filter((r) => r.status === "overdue").length;

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: yearStart,
      periodEnd: yearEnd,
      invoices,
      totalGross,
      totalOpen,
      overdueCount,
    };
  } finally {
    ctx.db.close();
  }
}

// --------------------------------------------------------------------------
// Per-company contacts (Kontakter — customers + vendors) — cockpit-redesign it. 5
// --------------------------------------------------------------------------

/** One customer in the master data. */
export type ContactCustomerRow = {
  id: number;
  name: string;
  vatOrCvr: string | null;
  email: string | null;
  paymentTermsDays: number;
  defaultCurrency: string;
  // #390 — surface the remaining stamdata fields so the Cockpit edit-modal can
  // prefill them without a second round-trip.
  address: string | null;
  phone: string | null;
  website: string | null;
  eanNumber: string | null;
  notes: string | null;
};

/** One vendor (supplier) in the master data. */
export type ContactVendorRow = {
  id: number;
  name: string;
  vatOrCvr: string | null;
  defaultExpenseAccount: string | null;
  defaultVatTreatment: string | null;
  // #390 — surface the remaining stamdata fields so the Cockpit edit-modal can
  // prefill them without a second round-trip.
  address: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  notes: string | null;
};

export type CompanyContacts = ReturnType<typeof buildCompanyContacts>;

/**
 * Kontakter — the company's customers and vendors (master data). This is
 * reference data, not year-scoped; the company sub-nav still carries the
 * selected `?year=` so it follows the user across views, so the fiscal years
 * for the selector are fetched alongside. Both lists come straight from
 * `core/master-data`. A company with no contacts returns empty lists — a
 * correct, expected state.
 */
export function buildCompanyContacts(workspaceRoot: string, slug: string) {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }

  const years = buildCompanyFiscalYears(workspaceRoot, slug).years;

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);

    const customers: ContactCustomerRow[] = listCustomers(db).rows.map((c) => ({
      id: c.id,
      name: c.name,
      vatOrCvr: c.vatOrCvr,
      email: c.email,
      paymentTermsDays: c.paymentTermsDays,
      defaultCurrency: c.defaultCurrency,
      address: c.address,
      phone: c.phone,
      website: c.website,
      eanNumber: c.eanNumber,
      notes: c.notes,
    }));
    const vendors: ContactVendorRow[] = listVendors(db).rows.map((v) => ({
      id: v.id,
      name: v.name,
      vatOrCvr: v.vatOrCvr,
      defaultExpenseAccount: v.defaultExpenseAccount,
      defaultVatTreatment: v.defaultVatTreatment,
      address: v.address,
      email: v.email,
      phone: v.phone,
      website: v.website,
      notes: v.notes,
    }));

    return {
      slug: entry.slug,
      company: statementCompanyBlock(company),
      fiscalYears: years,
      customers,
      vendors,
    };
  } finally {
    db.close();
  }
}

// --------------------------------------------------------------------------
// Per-company obligations (Forpligtelser — what the company owes, year-aware)
// — cockpit-redesign Runde 2, iteration 7
// --------------------------------------------------------------------------

/** One thing the company owes — a payable surfaced from the ledger. */
export type ObligationRow = {
  /** A short, stable key for the obligation kind. */
  kind:
    | "vat"
    | "corporation-tax"
    | "annual-report"
    | "creditors"
    | "auditor"
    | "other";
  /** A human Danish label, e.g. "Moms — Q2 2026". */
  label: string;
  /** The amount owed, kroner; positive is payable. */
  amount: number;
  /** The filing/payment deadline as YYYY-MM-DD, or null when none is known. */
  dueDate: string | null;
  /** Signed countdown from today to `dueDate`; null when `dueDate` is null. */
  daysRemaining: number | null;
  /** The ledger account the figure was read from, when one applies. */
  accountNo: string | null;
};

export type CompanyObligations = ReturnType<typeof buildCompanyObligations>;

/**
 * Standard Danish liability account numbers (the Dinero chart) whose meaning
 * is well-known enough to label precisely and — for VAT and corporation tax —
 * carry a derived deadline. Trade creditors and accrued auditor have no
 * statutory date the ledger can derive, so their `dueDate` is left null.
 */
const KNOWN_LIABILITY_ACCOUNTS: Record<
  string,
  { kind: ObligationRow["kind"]; label: string }
> = {
  "63000": { kind: "creditors", label: "Kreditorer (leverandørgæld)" },
  "63040": { kind: "auditor", label: "Afsat revisor" },
  "63060": { kind: "corporation-tax", label: "Skyldig selskabsskat" },
};

/**
 * The credit-signed balance (credit − debit, kroner) of every `liability`-type
 * account at `asOfDate`, excluding the entire standard Danish VAT block.
 *
 * No VAT account may appear as a liability row here — VAT is surfaced as its
 * own single obligation from the booked VAT position (`vatPositionForPeriod`),
 * and that net figure already represents the *whole* VAT obligation. The gross
 * VAT accounts (output VAT `64000`, foreign-services reverse-charge `64040`,
 * input VAT `64060`, …) are merely *components* of that computation, so
 * counting them here as well would double-count VAT. The exclusion uses the
 * same VAT-account identification as `vatPositionForPeriod`: `type = 'vat'`
 * (native-Rentemester chart) or the standard Danish block `64000`–`64099`.
 * The `64100`-block settlement accounts (`Momsafregning`) only shuttle money
 * between the VAT accounts and the bank, so they are excluded too.
 */
function liabilityBalancesAsOf(
  db: import("bun:sqlite").Database,
  asOfDate: string,
): Array<{ accountNo: string; name: string; balance: number }> {
  const rows = db
    .query(
      `SELECT a.account_no AS accountNo,
              a.name       AS name,
              COALESCE(SUM(jl.credit_amount - jl.debit_amount), 0) AS balance
         FROM accounts a
         JOIN journal_lines jl     ON jl.account_id = a.id
         JOIN journal_entries je   ON je.id = jl.journal_entry_id
        WHERE a.type = 'liability'
          AND je.status = 'posted'
          AND je.transaction_date <= ?
          AND a.type != 'vat'
          AND NOT (a.account_no >= '64000' AND a.account_no < '64100')
          AND lower(a.name) NOT LIKE '%momsafregning%'
          AND a.account_no NOT GLOB '641[0-9][0-9]'
        GROUP BY a.id
        ORDER BY a.account_no ASC`,
    )
    .all(asOfDate) as Array<{
    accountNo: string;
    name: string;
    balance: number;
  }>;
  return rows.map((r) => ({
    accountNo: r.accountNo,
    name: r.name,
    balance: roundKroner(r.balance),
  }));
}

/**
 * Forpligtelser — "what does the company owe, and when". A year-aware list of
 * the company's outstanding payables, each with the amount owed and a due date
 * where one is derivable. Every figure is read straight from the posted
 * ledger:
 *
 *  - VAT — the booked quarterly VAT position (`vatPositionForPeriod`); its
 *    deadline is the statutory filing date (`vatQuarterDeadline`).
 *  - Corporation tax, trade creditors, accrued auditor and any other payable
 *    — the credit balance of the `liability`-type accounts at the year end.
 *    Known account numbers get a precise Danish label; corporation tax also
 *    gets a derived SKAT deadline. The rest carry no date — that is fine and
 *    shown as "—" in the UI.
 *
 * Rows are returned sorted by due date (soonest first); rows with no date sink
 * to the bottom. Money is kroner. Throws `ApiError.notFound` when the slug is
 * not registered or has no ledger.
 */
export function buildCompanyObligations(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    if (ctx.isArchivedOnly) {
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        company: companyBlock,
        fiscalYears: ctx.years,
        obligations: [] as ObligationRow[],
        totalOwed: 0,
      };
    }

    const today = todayIsoDate();
    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearEnd = `${yearNum}-12-31`;
    const obligations: ObligationRow[] = [];

    // VAT: each VAT period settles separately. Surface every period that
    // carries a payable so the owner sees each filing deadline; if no period
    // has a payable, no VAT obligation is shown. #299: the periods follow the
    // company's real VAT cadence (`vatPeriodType`) — a monthly filer sees up to
    // twelve VAT lines, a half-yearly filer two — never a hardcoded quarter.
    for (const window of vatPeriodsForYear(yearNum, ctx.company.vatPeriodType)) {
      const position = vatPositionForPeriod(ctx.db, window.start, window.end);
      if (position.payable > 0) {
        obligations.push({
          kind: "vat",
          label: `Moms — ${vatPeriodLabel(window)}`,
          amount: position.payable,
          dueDate: window.filingDeadline,
          daysRemaining: daysBetween(today, window.filingDeadline),
          accountNo: null,
        });
      }
    }

    // Annual report (årsrapport) — the statutory filing to Erhvervsstyrelsen.
    // It is not a ledger payable (it has no amount owed), but it is the other
    // recurring legal deadline an owner must not miss, so the Forpligtelser
    // screen surfaces it alongside VAT (#290). The deadline is computed the
    // SAME way `agent run` does (`src/agent/loop.ts#checkDeadlines`): a
    // class-B company files its årsrapport by the 1st of the 5th month after
    // the fiscal year ends. The fiscal year is derived from the company's own
    // `fiscalYearStartMonth` / label strategy, so a non-calendar year is
    // handled correctly. `amount` is 0 — it is a deadline, not a debt.
    const fy = fiscalYearForDate(
      yearEnd,
      ctx.company.fiscalYearStartMonth,
      ctx.company.fiscalYearLabelStrategy,
    );
    const fyEndYear = parseInt(fy.end.slice(0, 4), 10);
    const fyEndMonth = parseInt(fy.end.slice(5, 7), 10);
    const annualReportDue = new Date(Date.UTC(fyEndYear, fyEndMonth + 4, 1));
    const annualReportDueDate = `${annualReportDue.getUTCFullYear()}-${String(
      annualReportDue.getUTCMonth() + 1,
    ).padStart(2, "0")}-01`;
    obligations.push({
      kind: "annual-report",
      label: `Årsrapport — regnskabsår ${fy.displayLabel}`,
      amount: 0,
      dueDate: annualReportDueDate,
      daysRemaining: daysBetween(today, annualReportDueDate),
      accountNo: null,
    });

    // Liability accounts with a credit balance — corporation tax, trade
    // creditors, accrued auditor and anything else. A debit (negative) balance
    // is not a payable, so it is skipped. Corporation tax for an income year
    // is due to SKAT on 1 November of the following year (the standard ApS
    // restskat deadline) — the only liability date the ledger can derive.
    for (const acc of liabilityBalancesAsOf(ctx.db, yearEnd)) {
      if (acc.balance <= 0) continue;
      const known = KNOWN_LIABILITY_ACCOUNTS[acc.accountNo];
      const kind: ObligationRow["kind"] = known?.kind ?? "other";
      const label = known?.label ?? acc.name;
      const dueDate =
        kind === "corporation-tax" ? `${yearNum + 1}-11-01` : null;
      obligations.push({
        kind,
        label,
        amount: acc.balance,
        dueDate,
        daysRemaining: dueDate === null ? null : daysBetween(today, dueDate),
        accountNo: acc.accountNo,
      });
    }

    // Sorted by due date, soonest first; dateless rows sink to the bottom.
    // Ties break by descending amount, then by label — fully deterministic.
    obligations.sort((a, b) => {
      if (a.dueDate !== b.dueDate) {
        if (a.dueDate === null) return 1;
        if (b.dueDate === null) return -1;
        return a.dueDate.localeCompare(b.dueDate);
      }
      if (a.amount !== b.amount) return b.amount - a.amount;
      return a.label.localeCompare(b.label, "da");
    });

    const totalOwed = roundKroner(
      obligations.reduce((acc, o) => acc + o.amount, 0),
    );

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      company: companyBlock,
      fiscalYears: ctx.years,
      obligations,
      totalOwed,
    };
  } finally {
    ctx.db.close();
  }
}

// --------------------------------------------------------------------------
// Per-company cash flow (Likviditet — actual money in/out, year-aware)
// — cockpit-redesign Runde 2, iteration 8
// --------------------------------------------------------------------------

/** One calendar month of actual money movement on the bank statement. */
export type CashflowMonth = {
  /** 1–12. */
  month: number;
  label: string;
  /** Sum of positive `bank_transactions.amount` in the month, kroner. */
  indbetalinger: number;
  /** Sum of negative amounts as a positive figure (money out), kroner. */
  udbetalinger: number;
  /** indbetalinger − udbetalinger; the net movement, kroner. */
  netto: number;
};

/** One dated point on the real bank-balance trajectory. */
export type CashflowBalancePoint = {
  date: string;
  /** The imported `balance_after` at this point, kroner. */
  balance: number;
};

export type CompanyCashflow = ReturnType<typeof buildCompanyCashflow>;

/**
 * Likviditet / pengestrøm — actual money in and out of the bank for the
 * selected calendar fiscal year, computed straight from the imported
 * `bank_transactions` (NOT the accrual ledger). This is what the owner's bank
 * app shows: real cash, not booked revenue.
 *
 *  - `months` — per-month indbetalinger (positive amounts) and udbetalinger
 *    (negative amounts, shown as a positive figure), all twelve months.
 *  - `balanceSeries` — the `balance_after` trajectory: every transaction in the
 *    year that carries a running balance, oldest-first; the real bank-balance
 *    line.
 *  - summary — opening balance (the actual balance the day before the year
 *    starts), total in, total out and closing balance for the year.
 *
 * `hasTransactions` is false when the company has no bank rows at all in the
 * year — the UI renders a clean empty state. Money is kroner. Throws
 * `ApiError.notFound` when the slug is not registered or has no ledger.
 */
export function buildCompanyCashflow(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;
    const priorYearEnd = `${yearNum - 1}-12-31`;

    const emptyMonths = (): CashflowMonth[] =>
      MONTH_NAMES_DK.map((label, i) => ({
        month: i + 1,
        label,
        indbetalinger: 0,
        udbetalinger: 0,
        netto: 0,
      }));

    if (ctx.isArchivedOnly) {
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: yearStart,
        periodEnd: yearEnd,
        hasTransactions: false,
        months: emptyMonths(),
        balanceSeries: [] as CashflowBalancePoint[],
        openingBalance: null as number | null,
        closingBalance: null as number | null,
        totalIn: 0,
        totalOut: 0,
      };
    }

    // Every bank transaction in the year, oldest-first — drives both the
    // monthly in/out totals and the balance trajectory.
    const rows = ctx.db
      .query(
        `SELECT bt.transaction_date AS date,
                bt.amount           AS amount,
                bt.balance_after    AS balanceAfter
           FROM bank_transactions bt
          WHERE bt.transaction_date >= ? AND bt.transaction_date <= ?
          ORDER BY bt.transaction_date ASC, bt.id ASC`,
      )
      .all(yearStart, yearEnd) as Array<{
      date: string;
      amount: number;
      balanceAfter: number | null;
    }>;

    const months = emptyMonths();
    let totalIn = 0;
    let totalOut = 0;
    const balanceSeries: CashflowBalancePoint[] = [];
    for (const r of rows) {
      const month = parseInt(r.date.slice(5, 7), 10);
      const slot = months[month - 1];
      const amount = Number(r.amount ?? 0);
      if (slot) {
        if (amount >= 0) slot.indbetalinger += amount;
        else slot.udbetalinger += -amount;
      }
      if (amount >= 0) totalIn += amount;
      else totalOut += -amount;
      if (r.balanceAfter !== null && r.balanceAfter !== undefined) {
        balanceSeries.push({
          date: r.date,
          balance: roundKroner(Number(r.balanceAfter)),
        });
      }
    }
    for (const m of months) {
      m.indbetalinger = roundKroner(m.indbetalinger);
      m.udbetalinger = roundKroner(m.udbetalinger);
      m.netto = roundKroner(m.indbetalinger - m.udbetalinger);
    }

    // Opening balance — the actual statement balance the day before the year
    // begins; closing balance — the actual balance at the year end. Both come
    // from the same `balance_after`-based helper the bank view uses, so they
    // are null when no statement carries a running balance.
    const openingBalance = actualBankBalanceAsOf(ctx.db, priorYearEnd);
    const closingBalance = actualBankBalanceAsOf(ctx.db, yearEnd);

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: yearStart,
      periodEnd: yearEnd,
      hasTransactions: rows.length > 0,
      months,
      balanceSeries,
      openingBalance,
      closingBalance,
      totalIn: roundKroner(totalIn),
      totalOut: roundKroner(totalOut),
    };
  } finally {
    ctx.db.close();
  }
}

// --------------------------------------------------------------------------
// Per-company recurring invoice templates
// --------------------------------------------------------------------------

/** One previously generated invoice for a template, ordered period-ascending. */
export type RecurringInvoiceGenerationRow = {
  id: number;
  periodIndex: number;
  invoiceNumber: string;
  issueDate: string;
  documentId: number;
  deliveryPeriodStart: string | null;
  deliveryPeriodEnd: string | null;
};

/** One recurring-invoice template plus the invoices it has already issued. */
export type RecurringInvoiceTemplateRow = {
  id: number;
  name: string;
  interval: "monthly" | "quarterly" | "yearly";
  firstIssueDate: string;
  /** Next date `generateRecurringInvoice` will materialize a new invoice for. */
  nextIssueDate: string;
  paymentTermsDays: number;
  deliveryPeriodMode: "issue_month" | "interval_window" | "none";
  notes: string | null;
  active: boolean;
  createdAt: string;
  generations: RecurringInvoiceGenerationRow[];
};

export type CompanyRecurringInvoices = {
  slug: string;
  templates: RecurringInvoiceTemplateRow[];
};

/**
 * The cockpit's read of the recurring-invoice templates plus a flat list of
 * every invoice each template has already produced. Retired templates are
 * included so a human can see history; the UI filters as needed.
 */
export function buildCompanyRecurringInvoices(
  workspaceRoot: string,
  slug: string,
): CompanyRecurringInvoices {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }

  const db = openDb(dbPath);
  try {
    migrate(db);
    const list = listRecurringInvoiceTemplates(db, { includeInactive: true });
    const templates: RecurringInvoiceTemplateRow[] = list.rows.map((row) => ({
      id: row.id,
      name: row.name,
      interval: row.interval,
      firstIssueDate: row.firstIssueDate,
      nextIssueDate: row.nextIssueDate,
      paymentTermsDays: row.paymentTermsDays,
      deliveryPeriodMode: row.deliveryPeriodMode,
      notes: row.notes,
      active: row.active,
      createdAt: row.createdAt,
      generations: listRecurringInvoiceGenerations(db, row.id).rows.map((g) => ({
        id: g.id,
        periodIndex: g.periodIndex,
        invoiceNumber: g.invoiceNumber,
        issueDate: g.issueDate,
        documentId: g.documentId,
        deliveryPeriodStart: g.deliveryPeriodStart,
        deliveryPeriodEnd: g.deliveryPeriodEnd,
      })),
    }));
    return { slug: entry.slug, templates };
  } finally {
    db.close();
  }
}
