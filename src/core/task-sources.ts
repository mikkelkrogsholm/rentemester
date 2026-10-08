import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { addDays, isValidIsoDate } from "./dates";
import { canonicalJson } from "./canonical-json";
import { companyPaths } from "./paths";
import { companyRootForSlug, findWorkspaceCompany } from "./workspace";
import { openLedgerReadOnly } from "./ledger-inspection";
import { listExceptions } from "./exceptions";
import { buildBookkeepingWorkbench, type WorkbenchRow } from "./bookkeeping-workbench";
import { buildBankReconciliationReport } from "./reconciliation";
import { computePeriodCloseReadiness } from "./period-close-readiness";
import { getBookkeepingBatchState } from "./bookkeeping-batch";
import { effectivePeriodState, normalizeVatPeriodType, vatPeriodLabel, vatPeriodsForYear, type AccountingPeriodStatus } from "./periods";
import { completeTask, listTasks, syncSourceTask, taskScopeCompanies, TaskError } from "./tasks";
import type { Task, TaskDraft, TaskPeriod, TaskReference, TaskSource, TaskSourceCheck } from "./tasks-types";

/** Exact coverage, also returned by the public task view. No universal compliance claim. */
export const SOURCE_COVERAGE: string[] = [
  "Åbne undtagelser, inklusive tidligere og lukkede perioder; bankrelaterede undtagelser samles med bankarbejdet.",
  "Alle sider af uafsluttet bogføringsarbejde fra workbench samt autoritativ bankafstemning; importeret cut-over respekteres.",
  "Registrerede regnskabsperioder og ikke-beståede kontroller for tidligere perioder; overlappende bank-/undtagelseskontroller samles.",
  "Momsangivelse for eksplicit registreret måned, kvartal eller halvår, også nul-/refusionsperioder. Afslutning kræver indberettet periode og reference.",
  "Seneste bogføringsrevision, der afventer godkendelse. Godkendelse læses fra batchens egne godkendelsesbeviser.",
  "Årsafslutning følger selskabets regnskabsår. Årsrapportpligt og frist kræver afklaring eller en eksplicit konfigureret rutine; ingen generel lovfrist antages.",
  "Momsfrister bruger produktets ML §57-regel, verificeret mod SKATs fristoversigt 2026/27 den 8. oktober 2026. Andre myndighedsfrister kræver særskilt vedligeholdt regelgrundlag.",
];

const VAT_BASIS = "Momsloven §57; SKAT: https://skat.dk/erhverv/moms/frister-indberet-og-betal-moms (verificeret 2026-10-08)";
const digest = (value: unknown) => createHash("sha256").update(canonicalJson(value)).digest("hex");
type PeriodRow = { id: number; period_start: string; period_end: string; kind: string; status: AccountingPeriodStatus; reference: string | null };
type SourceObservation = { draft: TaskDraft & { source: TaskSource }; check: TaskSourceCheck };
type SourceSummary = { created: number; updated: number; resolved: number; reopened: number; unknown: number; errors: Array<{ companySlug: string; reason: string }> };

function ledgerIdentity(db: Database): string {
  const row = db.query("SELECT ledger_uuid FROM ledger_identity WHERE id=1").get() as { ledger_uuid: string } | null;
  if (!row?.ledger_uuid || !/^[a-f0-9]{32}$/.test(row.ledger_uuid)) throw new Error("TASK_SOURCE_LEDGER_IDENTITY_UNAVAILABLE");
  return row.ledger_uuid;
}

function period(from: string, to: string): TaskPeriod { return { from, to, label: `${from} – ${to}` }; }
function companyReference(companySlug: string, kind: TaskReference["kind"], ref: string): TaskReference { return { companySlug, kind, ref }; }
function sourceHref(companySlug: string, kind: string, ref: string): string {
  const route = kind === "bank_work" ? "batchbogfoering" : kind === "exception" ? "undtagelser"
    : kind === "vat_filing" || kind === "vat_registration" ? "moms" : kind === "batch_approval" ? "batchbogfoering" : "periodelas";
  return `/companies/${companySlug}/${route}${kind === "batch_approval" ? `?runId=${encodeURIComponent(ref)}` : ""}`;
}

function observation(companySlug: string, uuid: string, input: {
  kind: string; ref: string; title: string; nextAction: string; content: unknown; check: TaskSourceCheck;
  period?: TaskPeriod; references?: TaskReference[]; deadline?: Task["deadline"]; type?: Task["type"]; relevance?: Task["relevance"]; origin?: Task["origin"];
}): SourceObservation {
  const source: TaskSource = { kind: input.kind, identity: `${uuid}:${input.kind}:${input.ref}`, companySlug,
    ref: input.ref, state: input.check.state, contentHash: digest(input.content), href: sourceHref(companySlug, input.kind, input.ref) };
  return { check: input.check, draft: { title: input.title, scope: { kind: "company", companySlug }, nextAction: input.nextAction,
    type: input.type ?? "ad_hoc", origin: input.origin ?? "system", source, period: input.period ?? null,
    references: input.references ?? [], deadline: input.deadline ?? null, evidenceRequired: true,
    relevance: input.relevance ?? "relevant", verificationRequired: input.check.state === "unknown" } };
}

function bankObservation(db: Database, companySlug: string, uuid: string, row: WorkbenchRow, observedAt: string): SourceObservation {
  const references: TaskReference[] = [companyReference(companySlug, "bank_transaction", String(row.bankTransactionId))];
  if (row.document) references.push(companyReference(companySlug, "document", String(row.document.id)));
  if (row.document?.party) references.push(companyReference(companySlug, "party", row.document.party.id));
  return observation(companySlug, uuid, { kind: "bank_work", ref: String(row.bankTransactionId),
    title: `Bogfør og afstem bankpost #${row.bankTransactionId}`,
    nextAction: row.status === "missingDocument" ? "Find bilaget på den beskyttede kildeside." : "Gennemgå den aktuelle bogføringsplan og dens dokumentation.",
    content: { sourceHash: row.sourceHash, state: checkBank(db, companySlug, String(row.bankTransactionId), observedAt).state },
    check: checkBank(db, companySlug, String(row.bankTransactionId), observedAt), references, period: period(row.date, row.date) });
}

function checkBank(db: Database, companySlug: string, ref: string, observedAt: string): TaskSourceCheck {
  if (!/^\d+$/.test(ref)) return { state: "unknown", observedAt, reason: "Bankreferencen er ugyldig." };
  const row = db.query("SELECT transaction_date FROM bank_transactions WHERE id=?").get(Number(ref)) as { transaction_date: string } | null;
  if (!row || !isValidIsoDate(row.transaction_date)) return { state: "unknown", observedAt, reason: "Bankposten kan ikke genfindes." };
  const report = buildBankReconciliationReport(db, row.transaction_date, row.transaction_date);
  if (!report.ok) return { state: "unknown", observedAt, reason: "Bankafstemningen kunne ikke kontrolleres." };
  const openExceptions = db.query("SELECT id FROM exceptions WHERE related_bank_transaction_id=? AND status='open' LIMIT 1").get(Number(ref));
  const match = report.matched.find(item => item.bankTransactionId === Number(ref));
  return { state: match && !openExceptions ? "resolved" : "open", observedAt,
    ...(match && !openExceptions ? { evidence: [companyReference(companySlug, "bank_transaction", ref)] } : {}) };
}

function checkVat(db: Database, companySlug: string, ref: string, observedAt: string): TaskSourceCheck {
  const [from, to] = ref.split(":");
  if (!isValidIsoDate(from ?? "") || !isValidIsoDate(to ?? "")) return { state: "unknown", observedAt, reason: "Momsperioden er ugyldig." };
  const row = db.query("SELECT id,status,reference FROM accounting_periods WHERE period_start=? AND period_end=? AND kind IN ('vat_period','vat_quarter') ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END,id DESC LIMIT 1")
    .get(from!, to!) as Pick<PeriodRow, "id" | "status" | "reference"> | null;
  if (row && effectivePeriodState(db, row.id, row.status) === "reported") {
    return row.reference?.trim() ? { state: "resolved", observedAt, evidence: [companyReference(companySlug, "period", ref)] }
      : { state: "unknown", observedAt, reason: "Perioden er markeret indberettet, men en kvitteringsreference mangler." };
  }
  const company = db.query("SELECT vat_period_type FROM companies ORDER BY id LIMIT 1").get() as { vat_period_type: string | null } | null;
  if (!company || company.vat_period_type === null || !normalizeVatPeriodType(company.vat_period_type)) {
    return { state: "unknown", observedAt, reason: "Periodens momsregistrering skal afklares; ophørt registrering afslutter ikke tidligere pligter." };
  }
  // Rubric evidence and VAT liability totals are not filing receipts.
  return { state: "open", observedAt };
}

function checkBatch(db: Database, companySlug: string, ref: string, observedAt: string): TaskSourceCheck {
  if (!/^\d+$/.test(ref)) return { state: "unknown", observedAt, reason: "Godkendelsesreferencen er ugyldig." };
  const batch = getBookkeepingBatchState(db, Number(ref));
  if (!batch) return { state: "unknown", observedAt, reason: "Bogføringsplanen kan ikke genfindes." };
  const revisions = batch.revisions as Array<{ revisionId: number; approvedAt: string | null }>;
  const latestId = revisions.at(-1)?.revisionId;
  if (latestId === undefined) return { state: "unknown", observedAt, reason: "En aktuel revision mangler." };
  const approved = revisions.some(item => item.revisionId === latestId && item.approvedAt);
  return { state: approved ? "resolved" : "open", observedAt,
    ...(approved ? { evidence: [companyReference(companySlug, "approval", `bookkeeping-batch:${ref}:${latestId}`)] } : {}) };
}

function checkPeriod(db: Database, companySlug: string, kind: string, ref: string, observedAt: string): TaskSourceCheck {
  const [from, to, code] = ref.split(":");
  if (!isValidIsoDate(from ?? "") || !isValidIsoDate(to ?? "")) return { state: "unknown", observedAt, reason: "Regnskabsperioden er ugyldig." };
  if (kind === "period_control") {
    const control = computePeriodCloseReadiness(db, { periodStart: from!, periodEnd: to!, cutoff: to! }).items.find(item => item.code === code);
    if (!control || control.status === "unavailable") return { state: "unknown", observedAt, reason: "Periodens kildekontrol er utilgængelig." };
    return { state: control.status === "passed" ? "resolved" : "open", observedAt,
      ...(control.status === "passed" ? { evidence: [companyReference(companySlug, "period", `${from}:${to}:${code}`)] } : {}) };
  }
  const rows = db.query("SELECT id,status,kind,reference FROM accounting_periods WHERE period_start=? AND period_end=? ORDER BY id DESC")
    .all(from!, to!) as Array<Pick<PeriodRow, "id" | "status" | "kind" | "reference">>;
  const row = kind === "annual_reporting" ? rows.find(item => item.kind === "fiscal_year") : rows[0];
  if (!row) return { state: "unknown", observedAt, reason: "Periodens afslutning skal dokumenteres." };
  const effective = effectivePeriodState(db, row.id, row.status);
  const resolved = kind === "annual_reporting" ? effective === "reported" && Boolean(row.reference?.trim()) : effective === "closed" || effective === "reported";
  return { state: resolved ? "resolved" : kind === "annual_reporting" ? "unknown" : "open", observedAt,
    ...(resolved ? { evidence: [companyReference(companySlug, "period", ref)] } : kind === "annual_reporting" ? { reason: "Årsrapportpligt og indsendelse skal afklares og dokumenteres." } : {}) };
}

function checkInLedger(db: Database, task: Pick<Task, "source">, observedAt: string): TaskSourceCheck {
  const source = task.source;
  if (!source) return { state: "unknown", observedAt, reason: "Opgaven har ingen autoritativ kilde." };
  const uuid = ledgerIdentity(db);
  if (source.identity !== `${uuid}:${source.kind}:${source.ref}`) return { state: "unknown", observedAt, reason: "Kildens ledger-identitet er ændret." };
  if (source.kind === "bank_work") return checkBank(db, source.companySlug, source.ref, observedAt);
  if (source.kind === "vat_filing") return checkVat(db, source.companySlug, source.ref, observedAt);
  if (source.kind === "batch_approval") return checkBatch(db, source.companySlug, source.ref, observedAt);
  if (["period_close", "period_control", "annual_reporting"].includes(source.kind)) return checkPeriod(db, source.companySlug, source.kind, source.ref, observedAt);
  if (source.kind === "exception") {
    const row = db.query("SELECT status FROM exceptions WHERE id=?").get(Number(source.ref)) as { status: string } | null;
    if (!row) return { state: "unknown", observedAt, reason: "Undtagelsen kan ikke genfindes." };
    return { state: row.status === "resolved" ? "resolved" : "open", observedAt };
  }
  return { state: "unknown", observedAt, reason: "Kildens relevans kræver afklaring." };
}

export function checkTaskSource(workspaceRoot: string, task: Task): TaskSourceCheck {
  const observedAt = new Date().toISOString();
  if (!task.source || !taskScopeCompanies(task.scope).includes(task.source.companySlug) || !findWorkspaceCompany(workspaceRoot, task.source.companySlug)) {
    return { state: "unknown", observedAt, reason: "Kildens selskab kan ikke genfindes." };
  }
  let db: Database | undefined;
  try {
    db = openLedgerReadOnly(companyPaths(companyRootForSlug(workspaceRoot, task.source.companySlug)).db);
    return checkInLedger(db, task, observedAt);
  } catch { return { state: "unknown", observedAt, reason: "Den autoritative kilde er utilgængelig; verificér resultatet før afslutning eller genforsøg." }; }
  finally { db?.close(); }
}

function collectSources(db: Database, companySlug: string, asOfDate: string): SourceObservation[] {
  const observedAt = new Date().toISOString();
  const uuid = ledgerIdentity(db);
  const observations = new Map<string, SourceObservation>();
  const add = (item: SourceObservation) => { observations.set(item.draft.source.identity, item); };
  const earliest = db.query("SELECT MIN(transaction_date) AS date FROM bank_transactions WHERE transaction_date<=?").get(asOfDate) as { date: string | null };
  if (earliest.date) {
    let cursor: number | undefined;
    do {
      const page = buildBookkeepingWorkbench(db, { from: earliest.date, to: asOfDate, limit: 100, cursor });
      if (page.state === "unavailable") throw new Error("WORKBENCH_SOURCE_UNAVAILABLE");
      for (const row of page.rows) add(bankObservation(db, companySlug, uuid, row, observedAt));
      cursor = page.page.nextCursor ?? undefined;
    } while (cursor !== undefined);
  }
  const exceptions = listExceptions(db, { status: "open", includeArchived: true });
  if (!exceptions.ok) throw new Error("EXCEPTION_SOURCE_UNAVAILABLE");
  for (const exception of exceptions.rows) {
    const bankRef = exception.relatedBankTransactionId ? String(exception.relatedBankTransactionId) : null;
    const bankKey = bankRef ? `${uuid}:bank_work:${bankRef}` : null;
    if (bankKey && observations.has(bankKey)) {
      const existing = observations.get(bankKey)!;
      existing.draft.source.contentHash = digest([existing.draft.source.contentHash, exception.id]);
      continue;
    }
    const refs: TaskReference[] = [];
    if (exception.relatedDocumentId) refs.push(companyReference(companySlug, "document", String(exception.relatedDocumentId)));
    if (bankRef) refs.push(companyReference(companySlug, "bank_transaction", bankRef));
    add(observation(companySlug, uuid, { kind: bankRef ? "bank_work" : "exception", ref: bankRef ?? String(exception.id),
      title: bankRef ? `Bogfør og afstem bankpost #${bankRef}` : `Afklar undtagelse #${exception.id}`,
      nextAction: "Læs undtagelsen og dens dokumentation på den beskyttede kildeside.",
      content: { id: exception.id, status: exception.status, severity: exception.severity }, references: refs,
      check: bankRef ? checkBank(db, companySlug, bankRef, observedAt) : { state: "open", observedAt } }));
  }
  const periods = db.query("SELECT id,period_start,period_end,kind,status,reference FROM accounting_periods WHERE period_start<=? ORDER BY period_start,id")
    .all(asOfDate) as PeriodRow[];
  for (const row of periods) {
    const range = period(row.period_start, row.period_end);
    const ref = `${range.from}:${range.to}`;
    if (effectivePeriodState(db, row.id, row.status) === "open") add(observation(companySlug, uuid, { kind: "period_close", ref,
      title: `Afslut regnskabsperioden ${range.label}`, nextAction: "Gennemgå og afslut perioden gennem periodens kontroller.",
      content: { row, effectiveState: "open" }, period: range, references: [companyReference(companySlug, "period", ref)], check: { state: "open", observedAt } }));
    const packet = computePeriodCloseReadiness(db, { periodStart: range.from, periodEnd: range.to, cutoff: range.to });
    for (const control of packet.items) {
      // These have specific canonical tasks above, and lifecycle belongs to period_close.
      if (control.status === "passed" || ["BANK_UNRECONCILED", "EXCEPTIONS_OPEN", "PERIOD_LIFECYCLE"].includes(control.code)) continue;
      add(observation(companySlug, uuid, { kind: "period_control", ref: `${ref}:${control.code}`, title: `Afklar periodekontrol: ${control.code}`,
        nextAction: "Gennemgå den konkrete kontrol og dens dokumentation på periodens kildeside.",
        content: { code: control.code, sourceHash: control.sourceHash, status: control.status }, period: range,
        references: [companyReference(companySlug, "period", ref)], check: { state: control.status === "unavailable" ? "unknown" : "open", observedAt } }));
    }
  }
  const company = db.query("SELECT vat_period_type,fiscal_year_start_month FROM companies ORDER BY id LIMIT 1").get() as { vat_period_type: string | null; fiscal_year_start_month: number } | null;
  if (!company) throw new Error("TASK_SOURCE_COMPANY_UNAVAILABLE");
  const cadence = normalizeVatPeriodType(company.vat_period_type);
  if (company.vat_period_type !== null && !cadence) {
    add(observation(companySlug, uuid, { kind: "vat_registration", ref: "registration", title: "Afklar momsregistrering og frister",
      nextAction: "Bekræft selskabets momsregistrering og afregningsperiode.", content: { cadence: "unknown" },
      relevance: "unknown", check: { state: "unknown", observedAt, reason: "Momsregistrering er ukendt." } }));
  }
  if (cadence) {
    const first = [earliest.date, ...periods.map(row => row.period_start)].filter((date): date is string => Boolean(date)).sort()[0] ?? `${asOfDate.slice(0, 4)}-01-01`;
    if (Number(asOfDate.slice(0, 4)) - Number(first.slice(0, 4)) > 100) throw new Error("TASK_SOURCE_PERIOD_RANGE_UNSUPPORTED");
    for (let year = Number(first.slice(0, 4)); year <= Number(asOfDate.slice(0, 4)); year++) {
      for (const window of vatPeriodsForYear(year, cadence)) {
        if (window.end < first || window.start > asOfDate) continue;
        const ref = `${window.start}:${window.end}`;
        const check = checkVat(db, companySlug, ref, observedAt);
        if (check.state === "resolved") continue;
        add(observation(companySlug, uuid, { kind: "vat_filing", ref, title: `Indberet moms: ${vatPeriodLabel(window)}`,
          nextAction: "Afstem perioden og gem den faktiske indberetningskvittering gennem momsforløbet.",
          content: { cadence, window, state: check.state }, type: "obligation", period: period(window.start, window.end),
          deadline: { date: window.filingDeadline, kind: "statutory", basis: VAT_BASIS, certainty: "confirmed", ruleId: "DK-VAT-ML57" },
          references: [companyReference(companySlug, "period", ref)], check }));
      }
    }
  }
  const annualRanges = new Map<string, TaskPeriod>();
  for (const row of periods.filter(item => item.kind === "fiscal_year")) annualRanges.set(`${row.period_start}:${row.period_end}`, period(row.period_start, row.period_end));
  const startMonth = company.fiscal_year_start_month;
  const asOfYear = Number(asOfDate.slice(0, 4));
  const fiscalStart = `${asOfYear - (Number(asOfDate.slice(5, 7)) < startMonth ? 1 : 0)}-${String(startMonth).padStart(2, "0")}-01`;
  const fiscalEnd = addDays(`${Number(fiscalStart.slice(0, 4)) + 1}-${String(startMonth).padStart(2, "0")}-01`, -1);
  annualRanges.set(`${fiscalStart}:${fiscalEnd}`, period(fiscalStart, fiscalEnd));
  for (const [ref, range] of annualRanges) {
    const check = checkPeriod(db, companySlug, "annual_reporting", ref, observedAt);
    if (check.state === "resolved") continue;
    add(observation(companySlug, uuid, { kind: "annual_reporting", ref, title: `Afklar årsrapport og årsafslutning: ${range.label}`,
      nextAction: "Bekræft årsrapportpligt, frist og dokumentation for det valgte regnskabsår.", content: { ref, state: check.state },
      type: "obligation", origin: "proposal", relevance: "unknown", period: range, check,
      references: [companyReference(companySlug, "period", ref)] }));
  }
  for (const row of db.query("SELECT id FROM bookkeeping_batch_runs ORDER BY id").all() as Array<{ id: number }>) {
    const check = checkBatch(db, companySlug, String(row.id), observedAt);
    if (check.state === "resolved") continue;
    const state = getBookkeepingBatchState(db, row.id);
    add(observation(companySlug, uuid, { kind: "batch_approval", ref: String(row.id), title: `Godkend bogføringsplan #${row.id}`,
      nextAction: "Gennemgå den seneste præcise revision i det beskyttede godkendelsesforløb.", content: { revisions: state?.revisions, state: check.state },
      references: [companyReference(companySlug, "approval", `bookkeeping-batch:${row.id}`)], check }));
  }
  return [...observations.values()];
}

/** Explicit sync only. A missing page or ledger never implies that a source is solved. */
export function syncTaskSources(db: Database, workspaceRoot: string, input: { companySlugs: string[]; actor: string; principal: string; asOfDate: string }): SourceSummary {
  if (!isValidIsoDate(input.asOfDate)) throw new TaskError("invalid_input", "Kildesynkronisering kræver en gyldig dato.");
  const result: SourceSummary = { created: 0, updated: 0, resolved: 0, reopened: 0, unknown: 0, errors: [] };
  for (const companySlug of [...new Set(input.companySlugs)]) {
    const entry = findWorkspaceCompany(workspaceRoot, companySlug);
    if (!entry) throw new TaskError("not_found", "Kildens selskab er ikke registreret i workspacet.");
    const allSources = listTasks(db, { showDone: true }).filter(task => task.source?.companySlug === companySlug);
    const existing = allSources.filter(task => taskScopeCompanies(task.scope).length > 0
      && taskScopeCompanies(task.scope).every(slug => input.companySlugs.includes(slug)));
    const outsideSelection = new Set(allSources.filter(task => !existing.includes(task)).map(task => task.source!.identity));
    const bySource = new Map(existing.map(task => [task.source!.identity, task]));
    let ledger: Database | undefined;
    let observations: SourceObservation[] = [];
    try {
      ledger = openLedgerReadOnly(companyPaths(companyRootForSlug(workspaceRoot, companySlug)).db);
      if (!entry.archived) observations = collectSources(ledger, companySlug, input.asOfDate).filter(item => !outsideSelection.has(item.draft.source.identity));
    } catch { result.errors.push({ companySlug, reason: "Kilder kunne ikke gennemlæses fuldstændigt; eksisterende arbejde er bevaret." }); }
    const observed = new Set(observations.map(item => item.draft.source.identity));
    // Every previously known source is rechecked independently, including completed tasks.
    for (const task of existing.filter(item => !observed.has(item.source!.identity))) {
      let check: TaskSourceCheck;
      const observedAt = new Date().toISOString();
      try { check = ledger ? checkInLedger(ledger, task, observedAt)
        : { state: "unknown", observedAt, reason: "Kilden er utilgængelig." }; }
      catch { check = { state: "unknown", observedAt, reason: "Kilden kunne ikke verificeres." }; }
      observations.push({ draft: { title: task.title, scope: task.scope, source: { ...task.source!, state: check.state,
        contentHash: task.source!.state === check.state ? task.source!.contentHash : digest({ previousContent: task.source!.contentHash, state: check.state }) },
        verificationRequired: check.state === "unknown" }, check });
    }
    ledger?.close();
    for (const observation of observations) {
      const before = bySource.get(observation.draft.source.identity);
      let after = syncSourceTask(db, observation.draft, { actor: input.actor, principal: input.principal,
        idempotencyKey: `source:${digest([observation.draft.source.identity, observation.draft.source.contentHash, observation.check.state, before?.version ?? 0])}`,
        expectedVersion: before?.version, sourceCheck: observation.check });
      if (after.status !== "done" && before?.source?.state !== "resolved" && observation.check.state === "resolved" && observation.check.evidence?.length) {
        after = completeTask(db, after.taskId, { outcome: "completed", note: "Kildens aktuelle resultat er registreret.", references: observation.check.evidence }, {
          actor: input.actor, principal: input.principal, expectedVersion: after.version,
          sourceCheck: observation.check,
          idempotencyKey: `source-complete:${digest([after.taskId, after.version, observation.check.evidence])}`,
        });
      }
      if (!before) result.created++;
      else if (after.version !== before.version) result.updated++;
      if (before?.status !== "done" && after.status === "done") result.resolved++;
      if (before?.status === "done" && after.status !== "done") result.reopened++;
      if (observation.check.state === "unknown") result.unknown++;
    }
  }
  return result;
}
