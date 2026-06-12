import {
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
} from "../shared";
import { archiveYearRow } from "../archive";

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
