import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { canonicalJson } from "./canonical-json";
import { addDays, isValidIsoDate } from "./dates";
import { createTask, executeTaskMutation, getTask, listTasks, taskId, taskContentText, TaskError, validateTaskDraft } from "./tasks";
import type { TaskMutationContext, TaskPeriod, TaskProjection, TaskScope, TaskSeries, TaskSeriesDraft } from "./tasks-types";

function selected(scope: TaskScope, companySlugs?: string[], includeWorkspace = false): boolean {
  if (scope.kind === "workspace" && scope.companySlugs.length === 0) return includeWorkspace;
  if (!companySlugs) return true;
  const slugs = scope.kind === "company" ? [scope.companySlug] : scope.companySlugs;
  return slugs.length > 0 && slugs.every(slug => companySlugs.includes(slug));
}

export function listTaskSeries(db: Database): TaskSeries[] {
  return (db.query("SELECT payload_json FROM rm_current_task_series ORDER BY series_id").all() as Array<{ payload_json: string }>)
    .map(row => JSON.parse(row.payload_json) as TaskSeries);
}

export function getTaskSeries(db: Database, seriesId: string): TaskSeries | null {
  const row = db.query("SELECT payload_json FROM rm_current_task_series WHERE series_id=?").get(seriesId) as { payload_json: string } | null;
  return row ? JSON.parse(row.payload_json) as TaskSeries : null;
}

function seriesVersions(db: Database, seriesId: string): TaskSeries[] {
  return (db.query("SELECT payload_json FROM rm_task_series_events WHERE series_id=? ORDER BY version").all(seriesId) as Array<{ payload_json: string }>)
    .map(row => JSON.parse(row.payload_json) as TaskSeries);
}

function validateSeries(input: TaskSeriesDraft): void {
  if (!input || typeof input !== "object" || !input.seriesId?.trim() || input.seriesId.length > 160
    || !input.title?.trim() || input.title.length > 240 || !["month", "quarter", "year", "custom"].includes(input.cadence)
    || !Number.isInteger(input.every) || input.every < 1 || input.every > 120
    || !["calendar", "fiscal"].includes(input.anchor) || !isValidIsoDate(input.startDate)
    || (input.endDate !== null && (!isValidIsoDate(input.endDate) || input.endDate < input.startDate))
    || !Number.isInteger(input.fiscalYearStartMonth) || input.fiscalYearStartMonth < 1 || input.fiscalYearStartMonth > 12
    || !Number.isInteger(input.workDayOffset) || Math.abs(input.workDayOffset) > 3660
    || (input.deadlineDayOffset !== null && (!Number.isInteger(input.deadlineDayOffset) || Math.abs(input.deadlineDayOffset) > 3660))
    || !["relevant", "unknown", "activity"].includes(input.relevance) || typeof input.active !== "boolean") {
    throw new TaskError("invalid_input", "Rutinen kræver gyldige perioder, kadence, titel og eksplicit scope.");
  }
  taskId(input.seriesId, "seriesId");
  validateTaskDraft(input.template);
  if (canonicalJson(input.scope) !== canonicalJson(input.template.scope)) throw new TaskError("invalid_input", "Rutinens scope og skabelonens scope skal være ens.");
  if (input.template.source || input.template.seriesId || input.template.occurrenceKey || input.template.taskId) {
    throw new TaskError("invalid_input", "Skabelonen må ikke genbruge identiteten fra en konkret opgave eller kilde.");
  }
  if (input.template.status === "done" || input.template.reminders?.some(reminder => reminder.enabled)) {
    throw new TaskError("invalid_input", "En rutineskabelon må ikke afslutte opgaver eller aktivere påmindelser automatisk.");
  }
  if (input.deadlineDayOffset !== null && input.template.deadline?.kind === "statutory" && !input.template.deadline.basis.trim()) {
    throw new TaskError("invalid_input", "En myndighedsfrist kræver et synligt regelgrundlag.");
  }
}

/** Changing a series appends a new template. Existing occurrences are never rewritten. */
export function saveTaskSeries(db: Database, input: TaskSeriesDraft, ctx: TaskMutationContext): TaskSeries {
  return executeTaskMutation(db, "series-save", input, ctx, now => {
    validateSeries(input);
    const current = getTaskSeries(db, input.seriesId);
    if (current ? ctx.expectedVersion !== current.version : ctx.expectedVersion !== undefined && ctx.expectedVersion !== 0) {
      throw new TaskError("version_conflict", "Rutinen er ændret; genindlæs den aktuelle version før redigering.");
    }
    if (current && canonicalJson(current.scope) !== canonicalJson(input.scope)) throw new TaskError("invalid_input", "En eksisterende rutine kan ikke flyttes til et andet scope.");
    const series: TaskSeries = { ...input, title: taskContentText(input.title, "series title", 240, true), version: (current?.version ?? 0) + 1, updatedAt: now };
    db.query("INSERT INTO rm_task_series_events(series_id,version,payload_json,actor,principal,created_at) VALUES(?,?,?,?,?,?)")
      .run(series.seriesId, series.version, canonicalJson(series), ctx.actor, ctx.principal, now);
    return series;
  });
}

function monthNumber(date: string): number { return Number(date.slice(0, 4)) * 12 + Number(date.slice(5, 7)) - 1; }
function firstDay(month: number): string { return new Date(Date.UTC(Math.floor(month / 12), month % 12, 1)).toISOString().slice(0, 10); }
function span(series: TaskSeries): number { return ({ month: 1, quarter: 3, year: 12, custom: 1 })[series.cadence] * series.every; }
function periodAnchor(series: TaskSeries): number {
  const startMonth = monthNumber(series.startDate);
  const fiscalMonth = series.anchor === "fiscal" ? series.fiscalYearStartMonth - 1 : 0;
  const year = Math.floor((startMonth - fiscalMonth) / 12);
  const base = year * 12 + fiscalMonth;
  return base + Math.floor((startMonth - base) / span(series)) * span(series);
}

export function taskOccurrenceId(seriesId: string, period: TaskPeriod): string {
  return `occurrence-${createHash("sha256").update(`${seriesId}:${period.from}:${period.to}`).digest("hex").slice(0, 32)}`;
}

function project(series: TaskSeries, month: number, db: Database): TaskProjection {
  const from = firstDay(month);
  const to = addDays(firstDay(month + span(series)), -1);
  const period = { from, to, label: `${from} – ${to}` };
  const projectionId = taskOccurrenceId(series.seriesId, period);
  const concrete = getTask(db, projectionId);
  const deadline = series.deadlineDayOffset === null ? null : {
    date: addDays(from, series.deadlineDayOffset),
    kind: series.template.deadline?.kind ?? "internal" as const,
    basis: series.template.deadline?.basis ?? "Den valgte rutines frist",
    certainty: series.template.deadline?.certainty ?? "unconfirmed" as const,
    ...(series.template.deadline?.ruleId ? { ruleId: series.template.deadline.ruleId } : {}),
  };
  return { projectionId, seriesId: series.seriesId, title: concrete?.title ?? series.title,
    scope: concrete?.scope ?? series.scope, period: concrete?.period ?? period, workDate: concrete?.workDate ?? addDays(from, series.workDayOffset),
    deadline: concrete?.deadline ?? deadline, relevance: series.relevance, concreteTaskId: concrete?.taskId ?? null };
}

/** A read-only annual-wheel projection, including concrete occurrences after a pause. */
export function projectTaskSeries(db: Database, input: { from: string; to: string; companySlugs?: string[]; includeWorkspace?: boolean }): TaskProjection[] {
  if (!isValidIsoDate(input.from) || !isValidIsoDate(input.to) || input.from > input.to
    || monthNumber(input.to) - monthNumber(input.from) > 1200) throw new TaskError("invalid_input", "Årshjulet kræver et ordnet datointerval på højst 100 år.");
  const projections: TaskProjection[] = [];
  for (const current of listTaskSeries(db).filter(item => selected(item.scope, input.companySlugs, input.includeWorkspace))) {
    const versions = seriesVersions(db, current.seriesId);
    for (let index = 0; index < versions.length; index++) {
      const series = versions[index]!;
      const effectiveFrom = index === 0 ? series.startDate : series.updatedAt.slice(0, 10);
      const nextFrom = versions[index + 1]?.updatedAt.slice(0, 10) ?? null;
      const months = span(series);
      const anchor = periodAnchor(series);
      const rangeStart = Math.max(anchor, anchor + Math.floor((monthNumber(input.from) - anchor) / months) * months);
      for (let month = rangeStart; month <= monthNumber(input.to); month += months) {
        const projection = project(series, month, db);
        if (projection.period.to < series.startDate || projection.period.to < input.from || projection.period.from > input.to) continue;
        const scheduled = series.active && projection.workDate >= series.startDate && projection.workDate >= effectiveFrom
          && (nextFrom === null || projection.workDate < nextFrom) && (series.endDate === null || projection.period.from <= series.endDate);
        if (scheduled) projections.push(projection);
      }
    }
  }
  const byId = new Map(projections.map(item => [item.projectionId, item]));
  const seriesById = new Map(listTaskSeries(db).map(item => [item.seriesId, item]));
  // A cadence/start-date edit must not hide the earlier concrete periods from the annual wheel.
  for (const task of listTasks(db, { showDone: true })) {
    if (!task.seriesId || !task.period || task.period.to < input.from || task.period.from > input.to
      || !selected(task.scope, input.companySlugs, input.includeWorkspace)) continue;
    const series = seriesById.get(task.seriesId);
    if (!series) continue;
    byId.set(task.taskId, { projectionId: task.taskId, seriesId: task.seriesId, title: task.title, scope: task.scope,
      period: task.period, workDate: task.workDate ?? task.period.from, deadline: task.deadline,
      relevance: task.relevance === "unknown" ? "unknown" : "relevant", concreteTaskId: task.taskId });
  }
  return [...byId.values()].sort((a, b) => a.period.from.localeCompare(b.period.from) || a.projectionId.localeCompare(b.projectionId));
}

/** Materializes due periods once; completion dates never become recurrence anchors. */
export function materializeTaskSeries(db: Database, input: { asOfDate: string; actor: string; principal: string; companySlugs?: string[]; includeWorkspace?: boolean }): { created: number; existing: number; taskIds: string[] } {
  if (!isValidIsoDate(input.asOfDate)) throw new TaskError("invalid_input", "Oprettelse af rutineforekomster kræver en gyldig dato.");
  const result = { created: 0, existing: 0, taskIds: [] as string[] };
  for (const current of listTaskSeries(db).filter(item => selected(item.scope, input.companySlugs, input.includeWorkspace))) {
    const versions = seriesVersions(db, current.seriesId);
    const startDate = versions.map(item => item.startDate).sort()[0]!;
    if (startDate > input.asOfDate) continue;
    for (const projection of projectTaskSeries(db, { from: startDate, to: input.asOfDate, companySlugs: input.companySlugs, includeWorkspace: input.includeWorkspace }).filter(item => item.seriesId === current.seriesId && selected(item.scope, input.companySlugs, input.includeWorkspace))) {
      if (projection.workDate > input.asOfDate) continue;
      if (projection.concreteTaskId) { result.existing++; result.taskIds.push(projection.concreteTaskId); continue; }
      const series = [...versions].reverse().find(item => item.version === 1 || item.updatedAt.slice(0, 10) <= projection.workDate)!;
      const task = createTask(db, { ...series.template, taskId: projection.projectionId, title: series.title, scope: series.scope,
        type: "routine", seriesId: series.seriesId, occurrenceKey: `${projection.period.from}:${projection.period.to}`,
        period: projection.period, workDate: projection.workDate, deadline: projection.deadline,
        // No activity source has been inspected: absence is never proof of irrelevance.
        relevance: series.relevance === "relevant" ? "relevant" : "unknown", reminders: [] }, {
        actor: input.actor, principal: input.principal, idempotencyKey: `materialize:${projection.projectionId}`,
      });
      result.created++; result.taskIds.push(task.taskId);
    }
  }
  return result;
}
