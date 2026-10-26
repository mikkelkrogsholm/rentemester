import type { Database, SQLQueryBindings } from "bun:sqlite";

export type AuditLogRow = {
  id: number;
  eventType: string;
  entityType: string;
  entityId: string | null;
  message: string;
  actor: string;
  createdAt: string;
};

/**
 * Returns the most recent audit_log entries sorted by created_at DESC, id DESC.
 *
 * Append-only table (enforced by triggers in schema.sql), so this is a safe
 * read-only query. Used by the dashboard render-engine and CLI summaries.
 */
export function listRecentAuditLog(db: Database, limit = 10): AuditLogRow[] {
  if (!Number.isInteger(limit) || limit <= 0) return [];
  const rows = db.query(
    `SELECT id, event_type, entity_type, entity_id, message, actor, created_at
       FROM audit_log
      ORDER BY created_at DESC, id DESC
      LIMIT ?`,
  ).all(limit) as Array<{
    id: number;
    event_type: string;
    entity_type: string;
    entity_id: string | null;
    message: string;
    actor: string;
    created_at: string;
  }>;
  return rows.map(mapAuditRow);
}

/** Options for {@link listAuditLog} — every filter is optional and ANDed. */
export type ListAuditLogOptions = {
  /** Max rows in the returned page. Caller is responsible for capping. */
  limit?: number;
  /** Rows to skip before the page. */
  offset?: number;
  /** YYYY-MM-DD or ISO timestamp. Includes rows created at or after this. */
  fromDate?: string;
  /** YYYY-MM-DD or ISO timestamp. Includes rows created at or before this. */
  toDate?: string;
  /** Case-insensitive substring filter on `event_type`. */
  eventTypeLike?: string;
  /** Case-insensitive substring filter on `actor`. */
  actorLike?: string;
};

export type ListAuditLogResult = {
  ok: true;
  /** Total matching rows across the entire filtered set (pre-pagination). */
  total: number;
  /** The current page of rows, sorted by created_at DESC, id DESC. */
  rows: AuditLogRow[];
  errors: string[];
};

/** A compact, cursor-based read model for a person's "Siden sidst" view.
 * It deliberately projects only recorded events: no advice or interpretation
 * is stored or inferred here. The cursor is the append-only audit id. */
export function listChangesSince(db: Database, afterId = 0, limit = 50) {
  const cursor = Number.isSafeInteger(afterId) && afterId > 0 ? afterId : 0;
  const cappedLimit = Math.min(Math.max(Math.floor(limit), 1), 100);
  const rows = db.query(
    `SELECT id, event_type, entity_type, entity_id, message, actor, created_at
       FROM audit_log
      WHERE id > ?
        AND (event_type LIKE 'journal_%' OR event_type LIKE 'document_%'
          OR event_type LIKE 'bank_%' OR event_type LIKE 'invoice_%'
          OR event_type LIKE 'exception_%' OR event_type LIKE 'vat_%'
          OR event_type LIKE 'period_%' OR event_type LIKE 'company_%')
      ORDER BY id ASC LIMIT ?`,
  ).all(cursor, cappedLimit) as Array<{ id:number; event_type:string; entity_type:string; entity_id:string|null; message:string; actor:string; created_at:string }>;
  const events = rows.map(mapAuditRow);
  return { events, cursor: events.at(-1)?.id ?? cursor };
}

/**
 * Filtered, paginated read of the audit_log table — the read side of the
 * append-only audit chain. Used by the cockpit's "Revisionsspor"-view and
 * the MCP `audit_log_list` tool so an agent can show its own work back to
 * the human user. No mutation paths exist for this table (#audit chain),
 * so callers can re-read freely.
 *
 * Order is always `created_at DESC, id DESC` (newest first), matching the
 * dashboard helper above. Filters AND together; an empty filter is the
 * full table.
 */
export function listAuditLog(
  db: Database,
  options: ListAuditLogOptions = {},
): ListAuditLogResult {
  const where: string[] = [];
  const params: SQLQueryBindings[] = [];
  if (typeof options.fromDate === "string" && options.fromDate.length > 0) {
    where.push("created_at >= ?");
    params.push(options.fromDate);
  }
  if (typeof options.toDate === "string" && options.toDate.length > 0) {
    // Inclusive upper bound — accept either YYYY-MM-DD or a full ISO
    // timestamp; SQLite's string compare is lex-correct for both.
    where.push("created_at <= ?");
    // Promote a bare date to end-of-day so the upper bound is inclusive of
    // the entire day in the typical human use case.
    const toBound =
      /^\d{4}-\d{2}-\d{2}$/.test(options.toDate)
        ? `${options.toDate}T23:59:59.999Z`
        : options.toDate;
    params.push(toBound);
  }
  if (typeof options.eventTypeLike === "string" && options.eventTypeLike.length > 0) {
    where.push("LOWER(event_type) LIKE ?");
    params.push(`%${options.eventTypeLike.toLowerCase()}%`);
  }
  if (typeof options.actorLike === "string" && options.actorLike.length > 0) {
    where.push("LOWER(actor) LIKE ?");
    params.push(`%${options.actorLike.toLowerCase()}%`);
  }
  const whereSql = where.length > 0 ? `WHERE ${where.join(" AND ")}` : "";

  const totalRow = db
    .query(`SELECT COUNT(*) AS n FROM audit_log ${whereSql}`)
    .get(...params) as { n: number };
  const total = totalRow?.n ?? 0;

  const rawLimit = typeof options.limit === "number" && Number.isFinite(options.limit)
    ? Math.max(0, Math.floor(options.limit))
    : 0;
  const limit = rawLimit > 0 ? rawLimit : total;
  const offset = typeof options.offset === "number" && Number.isFinite(options.offset)
    ? Math.max(0, Math.floor(options.offset))
    : 0;

  const rows = db
    .query(
      `SELECT id, event_type, entity_type, entity_id, message, actor, created_at
         FROM audit_log
         ${whereSql}
        ORDER BY created_at DESC, id DESC
        LIMIT ? OFFSET ?`,
    )
    .all(...params, limit, offset) as Array<{
      id: number;
      event_type: string;
      entity_type: string;
      entity_id: string | null;
      message: string;
      actor: string;
      created_at: string;
    }>;
  return {
    ok: true,
    total,
    rows: rows.map(mapAuditRow),
    errors: [],
  };
}

export type VerifyAuditLogIntegrityResult = {
  ok: boolean;
  /** Number of audit_log rows inspected. */
  rows: number;
  errors: string[];
};

/**
 * KODE-14 — minimal, deterministic tamper-evidence for the audit_log.
 *
 * audit_log is append-only (UPDATE/DELETE blocked by schema triggers), but
 * unlike journal_entries it has no hash chain or sequence guard, so a row
 * removed through a privileged path (a dropped trigger, a raw sqlite3 session,
 * a db file swapped in from outside) would leave no evidence. This is a
 * defence-in-depth check that detects *removed* rows two complementary ways:
 *
 *   1. id-gap detection. audit_log.id is a monotonic INTEGER PRIMARY KEY, so a
 *      clean log has a contiguous id range: COUNT(*) == MAX(id) - MIN(id) + 1.
 *      A hole means a middle row was deleted.
 *
 *   2. journal cross-check. Every journal entry is written together with its
 *      `journal_post` / `journal_reverse` audit event in the same transaction
 *      (see ledger.applyJournalEntry / reverseJournalEntry), both stamped with
 *      entity_type='journal_entry' and entity_id = the new entry id. So every
 *      journal entry MUST have at least one matching audit row. A missing one
 *      means an audit row was removed — this catches tail deletion, which
 *      leaves the id range contiguous and so slips past check (1). It also
 *      synergises with the period lifecycle replay (KODE-9), which trusts the
 *      audit_log as the source of truth.
 *
 * Intentionally minimal: it does not re-hash the log (no chain column exists)
 * — it proves *completeness against the journal*, which is the integrity that
 * actually matters for the ledger. It is read-only and deterministic.
 */
export type VerifyAuditLogIntegrityOptions = {
  /**
   * Cross-check every journal entry against its `journal_post` /
   * `journal_reverse` audit event. Strong, but it assumes every journal entry
   * was written through the normal posting path (ledger.applyJournalEntry /
   * reverseJournalEntry), which always logs that event in the same transaction.
   * Entries inserted by a raw INSERT that deliberately bypasses audit logging
   * (test fixtures, low-level migrations) would false-positive, so the caller
   * opts in. Defaults to true for the standalone integrity check.
   */
  journalCrossCheck?: boolean;
};

export function verifyAuditLogIntegrity(
  db: Database,
  options: VerifyAuditLogIntegrityOptions = {},
): VerifyAuditLogIntegrityResult {
  const journalCrossCheck = options.journalCrossCheck ?? true;
  const errors: string[] = [];

  const span = db
    .query("SELECT COUNT(*) AS n, MIN(id) AS lo, MAX(id) AS hi FROM audit_log")
    .get() as { n: number; lo: number | null; hi: number | null };
  const rows = Number(span.n ?? 0);

  if (rows > 0 && span.lo != null && span.hi != null) {
    const expectedSpan = Number(span.hi) - Number(span.lo) + 1;
    if (rows !== expectedSpan) {
      errors.push(
        `audit_log id sequence has gaps: ${rows} rows present but id range ${span.lo}..${span.hi} spans ${expectedSpan} — ${expectedSpan - rows} row(s) missing`,
      );
    }
  }

  // Journal cross-check: every journal entry must have its matching audit event.
  if (journalCrossCheck) {
    const orphanEntries = db
      .query(
        `SELECT je.id, je.entry_no
           FROM journal_entries je
          WHERE NOT EXISTS (
            SELECT 1 FROM audit_log al
             WHERE al.entity_type = 'journal_entry'
               AND al.entity_id = CAST(je.id AS TEXT)
          )
          ORDER BY je.id ASC`,
      )
      .all() as Array<{ id: number; entry_no: string }>;
    for (const entry of orphanEntries) {
      errors.push(`journal entry ${entry.entry_no} has no matching audit event — an audit_log row was removed`);
    }
  }

  return { ok: errors.length === 0, rows, errors };
}

function mapAuditRow(row: {
  id: number;
  event_type: string;
  entity_type: string;
  entity_id: string | null;
  message: string;
  actor: string;
  created_at: string;
}): AuditLogRow {
  return {
    id: row.id,
    eventType: row.event_type,
    entityType: row.entity_type,
    entityId: row.entity_id,
    message: row.message,
    actor: row.actor,
    createdAt: row.created_at,
  };
}
