import type { Database } from "bun:sqlite";

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
  const params: unknown[] = [];
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
