/**
 * MCP-response-envelope.
 *
 * Alle Rentemester-MCP-tools svarer med samme shape så agenten kan
 * stole på ét fælles kontrakt-format uafhængigt af tool-kategori:
 *
 *   { ok, data?, errors[], appliedRules? }
 *
 * Reglerne (jf. docs/mcp-tool-surface.md):
 *  - `ok=true` ⇒ `data` er sat, `errors` er tom array
 *  - `ok=false` ⇒ `errors` er ikke-tom array af strings, `data` udelades
 *  - `appliedRules` listes altid for bogførings-tools (sporbarhed)
 */

import { z } from "zod";

export type Envelope<TData = Record<string, unknown>> = {
  ok: boolean;
  data?: TData;
  errors: string[];
  appliedRules?: string[];
  /**
   * Stable, machine-readable error marker for cross-cutting preconditions
   * (`BACKUP_LOCKED`, …). Sættes kun ved `ok=false` og kun for fejl der
   * lever uden for det enkelte tools forretningslogik — så en agent kan
   * branche uden at parse dansk fri-tekst i `errors[]`. Per-tool
   * forretningsfejl bærer ikke `code` og forbliver kun beskrevet i
   * `errors[]`.
   */
  code?: string;
};

/**
 * Delt `outputSchema` for ALLE 81 MCP-tools (#202).
 *
 * Hver tool returnerer den samme `Envelope`-wrapper. Ved at deklarere
 * den som tool'ets `outputSchema` bliver wrapper-kontrakten
 * (`ok`/`errors`/`appliedRules`/`data`) maskin-kendt: en agent kan læse
 * resultatformen fra `tools/list` uden først at kalde tool'et.
 *
 * `data` er bevidst en åben (`passthrough`) objekt-form: den konkrete
 * per-tool `data`-shape varierer (`JournalPostResult`, `InvoiceListResult`
 * osv.) og er dokumenteret i docs/mcp-tool-surface.md frem for at blive
 * hånd-typet 81 gange. Schemaet siger derfor: "data er et objekt" — den
 * præcise feltliste står i kontrakt-dokumentet.
 *
 * Bemærk SDK-validering: MCP-SDK'en validerer kun `structuredContent`
 * mod dette schema for *succes*-svar (`isError:false`). Fejl-envelopes
 * (`isError:true`) springes over — derfor er `data` `.optional()`.
 */
export const envelopeShape = {
  ok: z
    .boolean()
    .describe("true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."),
  data: z
    .object({})
    .passthrough()
    .optional()
    .describe(
      "Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og " +
        "dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.",
    ),
  errors: z
    .array(z.string())
    .describe(
      "Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; " +
        "ikke-tom ved ok=false.",
    ),
  appliedRules: z
    .array(z.string())
    .optional()
    .describe(
      "Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for " +
        "bogførings-tools; udeladt ellers.",
    ),
  code: z
    .string()
    .optional()
    .describe(
      "Stabil maskinlæsbar fejl-markør for cross-cutting preconditions " +
        "(fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der " +
        "lever uden for det enkelte tools forretningslogik — agenten kan " +
        "branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt " +
        "ved per-tool forretningsfejl.",
    ),
} as const;

/**
 * `envelopeShape` som et zod-objekt — praktisk hvis man vil parse/validere
 * en envelope programmatisk.
 */
export const envelopeOutputSchema = z.object(envelopeShape);

/**
 * Wrapper et kerne-resultat i MCP-envelope-format.
 *
 * Kernens results har ofte shape `{ ok, errors, appliedRules, ...payload }`
 * — vi pakker `...payload` ud i `data` og bevarer `ok`, `errors`,
 * `appliedRules` på topniveau.
 */
export function wrapCoreResult<T extends { ok: boolean; errors?: unknown }>(
  result: T,
): Envelope {
  const { ok, errors, appliedRules, ...rest } = result as Record<string, unknown> & {
    ok: boolean;
    errors?: unknown;
    appliedRules?: unknown;
  };
  const envelope: Envelope = {
    ok: Boolean(ok),
    errors: normalizeErrors(errors),
  };
  if (Array.isArray(appliedRules) && appliedRules.length > 0) {
    envelope.appliedRules = appliedRules.map(String);
  }
  if (envelope.ok) envelope.data = rest;
  return envelope;
}

/**
 * Bygger en fejl-envelope uden at kalde kernen.
 *
 * Bruges fx når confirm-flag mangler, eller når input-validering fejler
 * før vi overhovedet rammer database/ledger.
 */
export function errorEnvelope(
  errors: string[] | string,
  options?: { code?: string },
): Envelope {
  const envelope: Envelope = {
    ok: false,
    errors: normalizeErrors(errors),
  };
  if (options?.code) envelope.code = options.code;
  return envelope;
}

/**
 * Bygger en succes-envelope direkte fra et data-objekt.
 */
export function successEnvelope<T extends Record<string, unknown>>(
  data: T,
  appliedRules?: string[],
): Envelope<T> {
  const envelope: Envelope<T> = {
    ok: true,
    data,
    errors: [],
  };
  if (appliedRules && appliedRules.length > 0) envelope.appliedRules = appliedRules.slice();
  return envelope;
}

/**
 * MCP `tools/call` skal returnere `{ content: [{ type: "text", text }] }`.
 * Vi serialiserer envelope'en som JSON og markerer `isError` så agenten
 * kan branche tidligt uden at parse body'en.
 */
export function envelopeToCallResult(envelope: Envelope): {
  content: Array<{ type: "text"; text: string }>;
  isError: boolean;
  structuredContent: Envelope;
} {
  return {
    content: [{ type: "text", text: JSON.stringify(envelope) }],
    isError: !envelope.ok,
    structuredContent: envelope,
  };
}

function normalizeErrors(value: unknown): string[] {
  if (!value) return [];
  if (Array.isArray(value)) return value.map((v) => String(v));
  return [String(value)];
}
