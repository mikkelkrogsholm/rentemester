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
  return rows.map((row) => ({
    id: row.id,
    eventType: row.event_type,
    entityType: row.entity_type,
    entityId: row.entity_id,
    message: row.message,
    actor: row.actor,
    createdAt: row.created_at,
  }));
}
