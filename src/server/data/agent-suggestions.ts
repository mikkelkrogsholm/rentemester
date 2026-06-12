// Agent-forslag → menneskelig godkendelse (#346) — read-side data for the
// cockpit's Suggestions view.
//
// Rentemester's narrative is: agent surfaces, human decides, ledger enforces.
// The agent loop (`src/agent/loop.ts`) and the exception sync functions in
// `src/core/exceptions.ts` already produce `AGENT_*` exceptions whenever the
// agent needs a human decision (overdue payable, accrual period due, possible
// fixed asset, tax-return needs-review). The cockpit's dashboard already shows
// these collapsed as a count on the "Opgaver"-card, but it does NOT let the
// owner see them individually, see the agent's rationale + the rule it cites,
// and approve or reject them one by one.
//
// This module surfaces every open `AGENT_*` exception as a structured
// agent-suggestion row, enriched with:
//   * the rule id from `source_evidence.rule` (e.g. "DK-PAYABLE-001")
//   * the actor that recorded it (so the audit trail shows WHICH agent)
//   * a Danish "kind" label so the table is readable without decoding the type
//   * the posting preview (when present) so a "godkend" click is informed
//
// It NEVER posts and NEVER mutates the ledger — approve/reject lives in
// `write-handlers.ts` and goes through `resolveException`. The list is purely
// derived from `exceptions` + `audit_log`, and is idempotent.

import { existsSync } from "node:fs";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { getCompanySettings } from "../../core/company";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../core/workspace";
import { ApiError } from "../errors";
import { statementCompanyBlock, type StatementCompanyBlock } from "./shared";

/**
 * The `AGENT_*` exception type prefix the agent loop + the exception sync
 * functions use for everything that is a "suggestion awaiting a human
 * decision". A future agent surface that adds a new suggestion class just
 * names its exception type `AGENT_<thing>` and this view picks it up — no
 * change here.
 */
const AGENT_TYPE_PREFIX = "AGENT_";

/**
 * The Danish label + cockpit deep-link target for each known agent-suggestion
 * type. Unknown `AGENT_*` types fall through to a generic "Agent-forslag" so a
 * new suggestion class is never silently dropped — the row still renders, just
 * without the tailored copy + deep link.
 */
type AgentTypeDescriptor = {
  /** Short Danish noun phrase shown in the "Type"-column. */
  kindLabel: string;
  /** Cockpit sub-view to deep-link to, or null when no sensible target. */
  link: string | null;
};

const AGENT_TYPE_DESCRIPTORS: Record<string, AgentTypeDescriptor> = {
  AGENT_PAYABLE_OVERDUE: {
    kindLabel: "Overforfalden kreditorpost",
    link: "leverandoerfaktura",
  },
  AGENT_ACCRUAL_RECOGNITION_DUE: {
    kindLabel: "Periodeafgrænsning klar til bogføring",
    link: "posteringer",
  },
  AGENT_TAX_RETURN_NEEDS_REVIEW: {
    kindLabel: "Oplysningsskema kræver gennemgang",
    link: null,
  },
  AGENT_POSSIBLE_FIXED_ASSET: {
    kindLabel: "Muligt anlæg — bør kapitaliseres",
    link: "anlaeg",
  },
};

function describeAgentType(type: string): AgentTypeDescriptor {
  return (
    AGENT_TYPE_DESCRIPTORS[type] ?? {
      kindLabel: "Agent-forslag",
      link: null,
    }
  );
}

/**
 * One agent suggestion the owner can approve or reject — a structured view of
 * an open `AGENT_*` exception with its rule id, the agent actor that recorded
 * it, the agent's rationale (the Danish message), and the recommended action.
 */
export type AgentSuggestionRow = {
  /** The underlying `exceptions.id`. Approve/reject targets this id. */
  exceptionId: number;
  /** The raw `exceptions.type` (e.g. `AGENT_PAYABLE_OVERDUE`). */
  type: string;
  /** Short Danish noun phrase — the table's "Type" cell. */
  kindLabel: string;
  /** `low` | `medium` | `high` — the urgency the sync function assigned. */
  severity: "low" | "medium" | "high";
  /**
   * The agent's rationale, in Danish. This is the exception's `message` — a
   * stable sentence that names the concrete subject (the bill, the period,
   * the year-end figure) without volatile day counts.
   */
  rationale: string;
  /** The Danish "what the human must do"-line. Null when none was recorded. */
  requiredAction: string | null;
  /** Rule id from `source_evidence.rule` (e.g. `DK-PAYABLE-001`). May be null. */
  ruleId: string | null;
  /**
   * Free-form `source_evidence` JSON the agent attached — already parsed, kept
   * so the cockpit can show the supporting figures (open balance, period
   * index, …) without re-deriving them.
   */
  sourceEvidence: unknown;
  /**
   * Free-form `posting_preview` JSON the agent attached — the deterministic
   * posting it would book if the owner approved. Null when none was recorded.
   */
  postingPreview: unknown;
  /** The agent actor that recorded the exception, e.g. `system:agent-loop`. */
  agentActor: string | null;
  /** The agent program (`created_by_program` in audit_log). */
  agentProgram: string | null;
  /** When the exception was recorded — when the agent first surfaced this. */
  createdAt: string;
  /** Related bilag (purchase / cash-register receipt), or null. */
  relatedDocumentId: number | null;
  /** Related bank transaction id, or null. */
  relatedBankTransactionId: number | null;
  /**
   * Deep-link target view inside the cockpit ("leverandoerfaktura", "anlaeg",
   * …) — null when the suggestion has no obvious target. The frontend appends
   * the company slug.
   */
  link: string | null;
};

export type CompanyAgentSuggestions = {
  slug: string;
  company: StatementCompanyBlock;
  /** Pending agent suggestions, severity DESC then id DESC (newest urgent first). */
  rows: AgentSuggestionRow[];
  /** Count of pending suggestions — same as `rows.length`, mirrored for headers. */
  count: number;
  /** Per-severity totals so the page can show a triage at a glance. */
  bySeverity: {
    high: number;
    medium: number;
    low: number;
  };
};

const SEVERITY_RANK: Record<string, number> = { low: 0, medium: 1, high: 2 };

function safeParseJson(raw: string | null): unknown {
  if (!raw) return null;
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

/**
 * Builds the pending agent-suggestion list for one company. Pulls every open
 * `AGENT_*` exception, joins it with the audit_log row that recorded it (so
 * the agent actor + program are surfaced), and enriches with the Danish kind
 * label + deep-link target.
 *
 * Idempotent + deterministic: re-running on the same ledger produces the same
 * rows in the same order — sorted by severity DESC, then by exception id DESC
 * (newest urgent first). Never opens a write transaction.
 */
export function buildCompanyAgentSuggestions(
  workspaceRoot: string,
  slug: string,
): CompanyAgentSuggestions {
  const entry = findWorkspaceCompany(workspaceRoot, slug);
  if (!entry) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const db = openDb(dbPath);
  try {
    migrate(db);
    const company = getCompanySettings(db);

    // Open agent-* exceptions, optionally joined to an audit_log entry that
    // attributes them to a specific agent actor. `audit_log` is the single
    // append-only attribution table (`entity_type`, `entity_id`, `actor`) but
    // the `recordException` core does not currently insert into it — the
    // attribution is best-effort and a LEFT JOIN keeps the row visible when
    // no audit entry exists. The agent-program field is derived from the
    // event_type / actor convention; the cockpit shows whichever fields are
    // populated and degrades gracefully on a legacy ledger.
    const rawRows = db
      .query(
        `SELECT e.id, e.type, e.severity, e.message, e.required_action,
                e.source_evidence, e.posting_preview, e.created_at,
                e.related_document_id, e.related_bank_transaction_id,
                al.actor AS agent_actor,
                al.event_type AS agent_event
         FROM exceptions e
         LEFT JOIN audit_log al
           ON al.entity_type = 'exception'
          AND al.entity_id = CAST(e.id AS TEXT)
         WHERE e.status = 'open'
           AND e.type LIKE ?
         ORDER BY e.id DESC`,
      )
      .all(`${AGENT_TYPE_PREFIX}%`) as Array<{
      id: number;
      type: string;
      severity: string;
      message: string;
      required_action: string | null;
      source_evidence: string | null;
      posting_preview: string | null;
      created_at: string;
      related_document_id: number | null;
      related_bank_transaction_id: number | null;
      agent_actor: string | null;
      agent_event: string | null;
    }>;

    const rows: AgentSuggestionRow[] = rawRows.map((row) => {
      const descriptor = describeAgentType(row.type);
      const evidence = safeParseJson(row.source_evidence);
      const ruleId =
        evidence && typeof evidence === "object" && evidence !== null
          ? typeof (evidence as { rule?: unknown }).rule === "string"
            ? (evidence as { rule: string }).rule
            : null
          : null;
      const severity =
        row.severity === "high" || row.severity === "medium" ? row.severity : "low";
      return {
        exceptionId: row.id,
        type: row.type,
        kindLabel: descriptor.kindLabel,
        severity,
        rationale: row.message,
        requiredAction: row.required_action,
        ruleId,
        sourceEvidence: evidence,
        postingPreview: safeParseJson(row.posting_preview),
        agentActor: row.agent_actor,
        agentProgram: row.agent_event,
        createdAt: row.created_at,
        relatedDocumentId: row.related_document_id,
        relatedBankTransactionId: row.related_bank_transaction_id,
        link: descriptor.link,
      };
    });

    // Severity DESC, then id DESC. SQLite already returned id DESC, so the
    // stable severity sort below is enough (Array.prototype.sort is stable).
    rows.sort(
      (a, b) =>
        (SEVERITY_RANK[b.severity] ?? 0) - (SEVERITY_RANK[a.severity] ?? 0),
    );

    const bySeverity = { high: 0, medium: 0, low: 0 };
    for (const row of rows) bySeverity[row.severity] += 1;

    return {
      slug: entry.slug,
      company: statementCompanyBlock(company),
      rows,
      count: rows.length,
      bySeverity,
    };
  } finally {
    db.close();
  }
}
