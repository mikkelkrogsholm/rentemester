// ===== GDPR (#184) =====
//
// CLI surface for the deterministic GDPR core: an export that gathers every
// record carrying a subject's personal data, plus a `forget` action that
// redacts the records the bookkeeping retention is no longer holding back.
//
// Three guarantees this layer enforces on top of the core:
//
// 1. **Erasure stays law-compliant.** `gdpr forget` requires the explicit
//    `--after-retention-expiry` flag, so the operator cannot run the command
//    without acknowledging that records still under bookkeeping retention will
//    be preserved by the core. The core already refuses individually-retained
//    records; this flag is the human-level seatbelt.
// 2. **Export is portable.** With `--out <dir>` the JSON export is written to
//    a stable filename inside the directory, so it can be handed to the data
//    subject as a single artifact — not piped through stdout.
// 3. **Subject naming is forgiving.** `--subject` is accepted as an alias for
//    `--cvr`; the docs use it because it reads better when the subject is a
//    Danish CVR identifier.
//
// The pre-existing `gdpr erase` command stays available unchanged for backwards
// compatibility with any scripts that already invoke it.

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { migrate } from "../core/db";
import {
  buildGdprSubjectExport,
  eraseGdprSubject,
  findGdprSubject,
} from "../core/gdpr";
import { openCommandDb } from "../cli-dispatch";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";

function readSubject(ctx: CommandContext): {
  cvr: string | null;
  name: string | null;
  asOf: string | null;
} {
  // `--subject` is the ROADMAP-facing alias; `--cvr` is the legacy name.
  return {
    cvr: ctx.arg("--cvr") ?? ctx.arg("--subject") ?? null,
    name: ctx.arg("--name") ?? null,
    asOf: ctx.arg("--as-of") ?? null,
  };
}

/** Build a stable filename for the on-disk export: deterministic, safe chars. */
function sanitizeSubjectKey(cvr: string | null, name: string | null): string {
  const raw = cvr ?? name ?? "subject";
  return raw.replace(/[^A-Za-z0-9_-]/g, "_");
}

/**
 * Registers `gdpr export`, `gdpr erase` (legacy) and `gdpr forget` (the
 * ROADMAP-facing name with an explicit retention-expiry flag).
 */
export function register(dispatch: CommandDispatch): void {
  dispatch.on("gdpr", "export", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildGdprSubjectExport(db, readSubject(ctx));
    const outDir = ctx.trimToNull(ctx.arg("--out"));
    let outPath: string | undefined;
    if (outDir && result.ok) {
      mkdirSync(outDir, { recursive: true });
      const subjectKey = sanitizeSubjectKey(result.subject.cvr, result.subject.name);
      const filename = `gdpr-export-${subjectKey}-${result.asOf}.json`;
      outPath = join(outDir, filename);
      writeFileSync(outPath, JSON.stringify(result, null, 2));
    }
    ctx.emitResult(
      outPath
        ? ({ ...result, outPath } as unknown as Record<string, unknown>)
        : (result as unknown as Record<string, unknown>),
    );
    db.close();
    if (!result.ok) process.exit(1);
  });

  // `gdpr discover` — subject-discovery på tværs af tabeller (#353).
  // Hurtigere end \`gdpr export\` fordi den ikke beriger med retention-status —
  // den lister bare hvor subject'et findes. Bruges når en agent eller en
  // ejer skal verificere "har vi overhovedet data om denne person?"
  dispatch.on("gdpr", "discover", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const result = findGdprSubject(db, readSubject(ctx));
    ctx.emitResult(result as unknown as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  // The redaction action: append-only audit, retention-aware in the core.
  function runErasure(ctx: CommandContext): void {
    const db = openCommandDb(ctx);
    migrate(db);
    const result = eraseGdprSubject(db, readSubject(ctx));
    ctx.emitResult(result as unknown as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  }

  // `gdpr erase` — legacy name, kept for back-compat with existing scripts.
  dispatch.on("gdpr", "erase", runErasure);

  // `gdpr forget` — the ROADMAP-facing name. Refuses without the explicit
  // `--after-retention-expiry` flag: a human-level seatbelt on top of the
  // core's per-record retention check. The flag does not change behaviour —
  // it makes the operator acknowledge the contract before pressing the lever.
  dispatch.on("gdpr", "forget", (ctx) => {
    if (!ctx.hasFlag("--after-retention-expiry")) {
      console.error(
        "Refused: `gdpr forget` requires --after-retention-expiry to confirm " +
          "that records still under bookkeeping retention will NOT be redacted. " +
          "The core enforces this per record; the flag is the operator's " +
          "explicit acknowledgement of the contract.",
      );
      process.exit(2);
    }
    runErasure(ctx);
  });
}
// ===== END GDPR (#184) =====
