import type { Database } from "bun:sqlite";

export type ActorContext = {
  createdBy: string;
  createdByProgram: string;
  auditActor: string;
};

export type ResolveActorInput = {
  createdBy?: string | null;
  createdByProgram?: string | null;
  fallbackActor?: {
    createdBy: string;
    createdByProgram?: string;
  };
};

function trimToNull(value: string | null | undefined) {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function inferredActorId() {
  return trimToNull(process.env.RENTEMESTER_ACTOR)
    ?? trimToNull(process.env.OPENCLAW_AGENT ? `agent:${process.env.OPENCLAW_AGENT}` : null)
    ?? trimToNull(process.env.RENTEMESTER_AGENT ? `agent:${process.env.RENTEMESTER_AGENT}` : null)
    ?? trimToNull(process.env.RENTEMESTER_USER ? `user:${process.env.RENTEMESTER_USER}` : null)
    ?? trimToNull(process.env.USER ? `user:${process.env.USER}` : null)
    ?? trimToNull(process.env.LOGNAME ? `user:${process.env.LOGNAME}` : null)
    ?? trimToNull(process.env.USERNAME ? `user:${process.env.USERNAME}` : null);  // Windows
}

function inferredActorVia() {
  return trimToNull(process.env.RENTEMESTER_ACTOR_VIA)
    ?? trimToNull(process.argv[1]?.includes("src/cli.ts") ? "rentemester-cli" : null)
    ?? "rentemester";
}

export function resolveActor(input: ResolveActorInput = {}): ActorContext {
  const createdBy = trimToNull(input.createdBy)
    ?? inferredActorId()
    ?? trimToNull(input.fallbackActor?.createdBy)
    ?? "system";
  const createdByProgram = trimToNull(input.createdByProgram)
    ?? trimToNull(input.fallbackActor?.createdByProgram)
    ?? inferredActorVia()
    ?? (createdBy === "system" ? "system" : "rentemester");

  const auditActor = createdBy === "system" && createdByProgram === "system"
    ? "system"
    : `${createdBy} via ${createdByProgram}`;

  return { createdBy, createdByProgram, auditActor };
}

export type AuditLogInput = {
  eventType: string;
  entityType: string;
  entityId?: string | number | null;
  message: string;
} & ResolveActorInput;

export function insertAuditLog(db: Database, input: AuditLogInput) {
  const actor = resolveActor(input);
  db.run(
    "INSERT INTO audit_log (event_type, entity_type, entity_id, message, actor) VALUES (?, ?, ?, ?, ?)",
    input.eventType,
    input.entityType,
    input.entityId == null ? null : String(input.entityId),
    input.message,
    actor.auditActor,
  );
}
