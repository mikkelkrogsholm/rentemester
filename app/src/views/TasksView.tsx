import { useState } from 'react';
import * as stylex from '@stylexjs/stylex';
import { Link, useLocation, useParams, useSearchParams } from 'react-router-dom';
import { Button, ButtonLink, Field, Input, PageHeader, Select } from '../components/ui';
import { ErrorState, Loading } from '../components/Feedback';
import { colors, spacing, rounded } from '../design/tokens.stylex';
import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { useTaskMutation } from '../lib/useTaskMutation';
import { boardFor, canWriteScope, outcomeLabel, referenceHref, scopeLabel, statusOptions, taskQuery, taskReturnTo, taskSourceHref, taskStatusLabel, taskTypeLabel, taskUrgency, today, type Task, type TaskProjection, type TaskScope, type TaskSeries, type TasksView as TasksData } from '../lib/tasks';
import { BoardEditor, CompletionEditor, ReminderEditor, SeriesEditor, TaskEditor, taskAssignees } from './TaskForms';

const styles = stylex.create({
  inlineLink: { textDecorationLine: 'underline' },
  page: { minWidth: 0, overflowWrap: 'anywhere' }, toolbar: { display: 'flex', flexWrap: 'wrap', gap: spacing.sm, alignItems: 'end', marginBottom: spacing.md },
  filters: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(min(100%, 190px), 1fr))', gap: spacing.sm, marginBlock: spacing.md },
  list: { display: 'grid', gap: spacing.sm, padding: 0, listStyle: 'none' }, card: { minWidth: 0, display: 'grid', gap: spacing.xs, padding: spacing.md, backgroundColor: colors.paperRaised, borderWidth: 1, borderStyle: 'solid', borderColor: colors.border, borderRadius: rounded.md },
  title: { margin: 0, fontSize: '18px', overflowWrap: 'anywhere' }, meta: { margin: 0, color: colors.inkMuted }, actions: { display: 'flex', flexWrap: 'wrap', gap: spacing.xs },
  board: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(min(100%, 220px), 1fr))', gap: spacing.md, alignItems: 'start' }, column: { minWidth: 0, backgroundColor: colors.paper, padding: spacing.sm, borderWidth: 1, borderStyle: 'solid', borderColor: colors.border, borderRadius: rounded.md },
  calendar: { display: 'grid', gridTemplateColumns: 'repeat(7, minmax(0, 1fr))', gap: spacing.xs, '@media (max-width: 639px)': { gridTemplateColumns: '1fr' } },
  date: { padding: spacing.xs, minWidth: 0, borderWidth: 1, borderStyle: 'solid', borderColor: colors.border, backgroundColor: colors.paperRaised }, dateList: { listStyle: 'none', padding: 0, display: 'grid', gap: spacing.xs },
  wheel: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(min(100%, 250px), 1fr))', gap: spacing.md }, detail: { display: 'grid', gridTemplateColumns: 'minmax(0, 1fr)', gap: spacing.md }, badge: { fontWeight: 600, color: colors.warning },
});
type ViewName = 'list' | 'calendar' | 'kanban' | 'wheel';
const tabs: Array<[ViewName, string]> = [['list', 'Liste'], ['calendar', 'Kalender'], ['kanban', 'Kanban'], ['wheel', 'Årshjul']];
function monthName(month: string) { return new Intl.DateTimeFormat('da-DK', { month: 'long', year: 'numeric', timeZone: 'UTC' }).format(new Date(`${month}-01T12:00:00Z`)); }
function dateAdd(date: string, days: number) { const parsed = new Date(`${date}T12:00:00Z`); parsed.setUTCDate(parsed.getUTCDate() + days); return parsed.toISOString().slice(0, 10); }
function validDate(value: string | null) { return value && /^\d{4}-\d{2}-\d{2}$/.test(value) && !Number.isNaN(new Date(`${value}T12:00:00Z`).getTime()) ? value : today(); }
function validYear(value: string | null) { return value && /^\d{4}$/.test(value) && Number(value) >= 1900 && Number(value) <= 9998 ? Number(value) : Number(today().slice(0, 4)); }
function calendarRange(date: string, mode: string) {
  if (mode === 'week') { const weekday = new Date(`${date}T12:00:00Z`).getUTCDay(); const from = dateAdd(date, -(weekday === 0 ? 6 : weekday - 1)); return { from, to: dateAdd(from, 6) }; }
  const from = `${date.slice(0, 7)}-01`; const last = new Date(`${from}T12:00:00Z`); last.setUTCMonth(last.getUTCMonth() + 1, 0);
  return { from, to: last.toISOString().slice(0, 10) };
}
function dateSequence(from: string, to: string) { const result: string[] = []; for (let cursor = from; cursor <= to && result.length < 370; cursor = dateAdd(cursor, 1)) result.push(cursor); return result; }

export function TasksView() {
  const { slug } = useParams();
  const location = useLocation();
  const [params, setParams] = useSearchParams();
  const listParams = new URLSearchParams(params); listParams.set('view', 'list');
  const listHref = `${location.pathname}?${listParams}`;
  const view = tabs.some(([tab]) => tab === params.get('view')) ? params.get('view') as ViewName : 'list';
  const calendarMode = ['month', 'week', 'agenda'].includes(params.get('calendar') ?? '') ? params.get('calendar')! : typeof window !== 'undefined' && window.matchMedia?.('(max-width: 639px)').matches ? 'agenda' : 'month';
  const date = validDate(params.get('date'));
  const wheelYear = validYear(params.get('wheelYear'));
  const wheelMonthRaw = Number(params.get('wheelStartMonth') ?? 1);
  const wheelMonth = Number.isInteger(wheelMonthRaw) && wheelMonthRaw >= 1 && wheelMonthRaw <= 12 ? wheelMonthRaw : 1;
  const range = view === 'wheel' ? { from: `${wheelYear}-${String(wheelMonth).padStart(2, '0')}-01`, to: dateAdd(`${wheelYear + 1}-${String(wheelMonth).padStart(2, '0')}-01`, -1) } : calendarRange(date, calendarMode);
  const query = { ...taskQuery(params, slug), ...(view === 'wheel' ? { showDone: true } : {}) };
  const key = JSON.stringify(query);
  const state = useAsync(async signal => {
    const base = await api.tasks(query, { signal });
    if (view === 'calendar' || view === 'wheel') {
      const projected = await api.tasks({ ...query, from: range.from, to: range.to }, { signal });
      return { ...base, projections: projected.projections };
    }
    return base;
  }, [slug, key, view, range.from, range.to]);
  const [editor, setEditor] = useState(false);
  const [seriesEditor, setSeriesEditor] = useState<TaskSeries | 'new' | null>(null);
  const [boardEditor, setBoardEditor] = useState(false);
  const [sourceErrors, setSourceErrors] = useState<Array<{ companySlug: string; reason: string }>>([]);
  const write = useTaskMutation(state.reload, 'sync');
  function setParam(key: string, value: string) { const next = new URLSearchParams(params); if (value) next.set(key, value); else next.delete(key); setParams(next, { replace: true }); }
  if (state.loading && !state.data) return <Loading label="Henter opgaver…" />;
  if (!state.data) return <ErrorState message={state.error ?? 'Opgaver kunne ikke hentes.'} onRetry={state.reload} />;
  const data = state.data;
  const board = boardFor(data, slug);
  const selectedCompanies = query.companySlugs ?? [];
  const initialScope: TaskScope | undefined = slug || selectedCompanies.length === 1 ? { kind: 'company', companySlug: slug ?? selectedCompanies[0]! } : undefined;
  const canCreate = data.companies.some(company => company.canWrite) || data.canManageWorkspace;
  const canManage = canWriteScope(board.scope, data, true);
  const syncCompanies = data.companies.filter(company => company.canWrite && !company.archived && (selectedCompanies.length === 0 || selectedCompanies.includes(company.slug))).map(company => company.slug);
  const focus = params.get('focus') ?? '';
  const externalAssignee = params.get('assigneeId')?.startsWith('external:') ? params.get('assigneeId')!.slice(9) : null;
  const externalNames = [...new Set(data.tasks.flatMap(task => task.assignee?.kind === 'external' ? [task.assignee.name] : []))];
  if (externalAssignee && !externalNames.includes(externalAssignee)) externalNames.push(externalAssignee);
  const filtered = data.tasks.filter(task => !externalAssignee || (task.assignee?.kind === 'external' && task.assignee.name === externalAssignee)).filter(task => focus === 'overdue' ? task.status !== 'done' && !!task.deadline && task.deadline.date < today() : focus === 'this_month' ? [task.workDate, task.deadline?.date].some(value => value?.slice(0, 7) === today().slice(0, 7)) : focus === 'waiting_me' ? task.status === 'waiting' && task.assignee?.kind === 'member' && task.assignee.userId === data.currentUserId : true);
  const tasks = [...filtered].sort((a, b) => (a.status === 'done' ? 1 : 0) - (b.status === 'done' ? 1 : 0) || (a.deadline?.date ?? a.workDate ?? '9999').localeCompare(b.deadline?.date ?? b.workDate ?? '9999') || a.title.localeCompare(b.title, 'da'));
  const returnTo = `${location.pathname}${location.search}`;
  const visibleSourceErrors = sourceErrors.filter(error => data.companies.some(company => company.slug === error.companySlug) && (selectedCompanies.length === 0 || selectedCompanies.includes(error.companySlug)));
  async function sync() {
    const result = await write.run<{ sync: { errors: Array<{ companySlug: string; reason: string }> } }>('/api/tasks/sync', { companySlugs: syncCompanies });
    if (result) setSourceErrors(result.sync.errors);
  }
  async function materialize() { await write.run('/api/task-series/materialize', { asOfDate: today(), ...(selectedCompanies.length > 0 ? { companySlugs: selectedCompanies } : {}) }); }
  return <section {...stylex.props(styles.page)} data-cockpit-page="tasks">
    <PageHeader title="Opgaver" description={slug ? `${data.companies.find(company => company.slug === slug)?.name ?? slug} · Planlægning og dokumenteret afslutning` : 'Samlet økonomiarbejde på tværs af dine selskaber'} actions={<>
      {canCreate && <Button onClick={() => setEditor(true)}>Opret opgave</Button>}
      {syncCompanies.length > 0 && <Button variant="secondary" busy={write.busy} disabled={write.blocked} onClick={() => void sync()}>Opdatér fra kilder</Button>}
    </>} />
    {write.feedback}{state.error && <p className="banner warning" role="alert">{state.error} Senest hentede data vises. Formularer bevares.</p>}
    {visibleSourceErrors.length > 0 && <div className="banner warning" role="alert"><h2>Kildegrundlaget er ufuldstændigt</h2><p>Disse selskabers kilder kunne ikke gennemlæses. En tom liste betyder ikke, at arbejdet er afsluttet. Kontrollér fejlen og opdatér fra kilder igen.</p><ul>{visibleSourceErrors.map(error => <li key={error.companySlug}>{data.companies.find(company => company.slug === error.companySlug)?.name}: {error.reason}</li>)}</ul></div>}
    {data.notifications.length > 0 && <details className="card"><summary>Påmindelser til dig ({data.notifications.length})</summary><ul>{data.notifications.map(notification => <li key={notification.notificationId}><Link to={`/opgaver/${encodeURIComponent(notification.taskId)}?${new URLSearchParams({ returnTo })}`}>{notification.title}</Link> · {notification.createdAt.slice(0, 10)}</li>)}</ul></details>}
    <nav aria-label="Opgavevisning" {...stylex.props(styles.toolbar)}>{tabs.map(([tab, label]) => <Button key={tab} variant={view === tab ? 'primary' : 'secondary'} aria-pressed={view === tab} onClick={() => setParam('view', tab)}>{label}</Button>)}</nav>
    <div {...stylex.props(styles.toolbar)} role="group" aria-label="Hurtigfiltre">{[['', 'Alle åbne'], ['overdue', 'Forfaldent'], ['this_month', 'Denne måned'], ['waiting_me', 'Afventer mig']].map(([value, label]) => <Button key={value} variant="secondary" aria-pressed={focus === value} onClick={() => setParam('focus', value!)}>{label}</Button>)}</div>
    <details className="card" open={!!params.get('search')}><summary>Filtre og selskabsvalg</summary><div {...stylex.props(styles.filters)}>
      {!slug && <fieldset><legend>Vis selskaber</legend>{data.companies.map(company => <label key={company.slug}><Input type="checkbox" checked={selectedCompanies.includes(company.slug)} onChange={event => { const next = new URLSearchParams(params); next.delete('companySlug'); const slugs = event.target.checked ? [...selectedCompanies, company.slug] : selectedCompanies.filter(value => value !== company.slug); for (const value of slugs) next.append('companySlug', value); setParams(next, { replace: true }); }} /> {company.name}{company.archived ? ' · arkiveret' : ''}<br /></label>)}<p className="muted">Intet valgt: alle selskaber med adgang.</p></fieldset>}
      <Field label="Søg titel eller reference"><Input type="search" value={params.get('search') ?? ''} onChange={event => setParam('search', event.target.value)} /></Field>
      <Field label="Statusfilter"><Select value={params.get('status') ?? ''} onChange={event => setParam('status', event.target.value)}><option value="">Alle statusser</option>{statusOptions.map(([status, label]) => <option key={status} value={status}>{label}</option>)}</Select></Field>
      <Field label="Typefilter"><Select value={params.get('type') ?? ''} onChange={event => setParam('type', event.target.value)}><option value="">Alle typer</option>{Object.entries(taskTypeLabel).map(([type, label]) => <option key={type} value={type}>{label}</option>)}</Select></Field>
      <Field label="Ansvarligfilter"><Select value={params.get('assigneeId') ?? ''} onChange={event => setParam('assigneeId', event.target.value)}><option value="">Alle ansvarlige</option>{taskAssignees(data).map(member => <option key={member.userId} value={member.userId}>{member.name}</option>)}{externalNames.map(name => <option key={`external:${name}`} value={`external:${name}`}>{name} · ekstern</option>)}</Select></Field>
      <Field label="Fra dato (eksplicit afgrænsning)"><Input type="date" value={params.get('from') ?? ''} onChange={event => setParam('from', event.target.value)} /></Field><Field label="Til dato"><Input type="date" value={params.get('to') ?? ''} onChange={event => setParam('to', event.target.value)} /></Field>
      {([['undated', 'Uden dato'], ['unassigned', 'Uden ansvarlig'], ['showDone', 'Vis afsluttede og historik'], ['includeArchived', 'Vis arkiverede selskabers opgaver']] as const).map(([key, label]) => <label key={key}><Input type="checkbox" checked={params.get(key) === 'true'} onChange={event => setParam(key, event.target.checked ? 'true' : '')} /> {label}</label>)}
      <Button variant="secondary" onClick={() => { const next = new URLSearchParams(); next.set('view', view); setParams(next, { replace: true }); }}>Nulstil filtre</Button>
    </div></details>
    <p role="status">{tasks.length} opgaver · {data.runtime.running ? 'Påmindelsesruntime kører' : 'Påmindelsesruntime er inaktiv'}</p>
    {view === 'list' && <TaskList tasks={tasks} data={data} returnTo={returnTo} onSaved={state.reload} />}
    {view === 'kanban' && <><div {...stylex.props(styles.toolbar)}><h2>Kanban</h2>{canManage && <Button variant="secondary" onClick={() => setBoardEditor(true)}>Tilpas kolonner</Button>}</div><div {...stylex.props(styles.board)}>{board.columns.map(column => {
      const columnTasks = tasks.filter(task => {
        const saved = slug ? task.columnId : task.workspaceColumnId;
        const target = board.columns.find(col => col.columnId === saved && col.status === task.status) ?? board.columns.find(col => col.status === task.status && col.isDefault) ?? board.columns.find(col => col.status === task.status);
        return target?.columnId === column.columnId;
      });
      return <section key={column.columnId} {...stylex.props(styles.column)} aria-label={`Kolonne ${column.name}`}><h2>{column.name} ({columnTasks.length})</h2><p className="muted">{taskStatusLabel[column.status]}{column.isDefault ? ' · standardvalg' : ''}</p><TaskList tasks={columnTasks} data={data} returnTo={returnTo} onSaved={state.reload} compact /></section>;
    })}</div></>}
    {view === 'calendar' && <><div {...stylex.props(styles.toolbar)}><Field label="Kalenderdato"><Input type="date" value={date} onChange={event => setParam('date', event.target.value)} /></Field><Field label="Kalendervisning"><Select value={calendarMode} onChange={event => setParam('calendar', event.target.value)}><option value="month">Måned</option><option value="week">Uge</option><option value="agenda">Agenda</option></Select></Field></div><h2>{calendarMode === 'week' ? `${range.from} – ${range.to}` : monthName(date.slice(0, 7))}</h2><Calendar tasks={tasks} projections={data.projections} data={data} returnTo={returnTo} range={range} mode={calendarMode} />
      <h2>Uden dato ({tasks.filter(task => !task.workDate && !task.deadline).length})</h2><TaskList tasks={tasks.filter(task => !task.workDate && !task.deadline)} data={data} returnTo={returnTo} onSaved={state.reload} />
      <p>Arbejdsdato og frist vises særskilt. Hver opgave tæller én gang i oversigten. <Link {...stylex.props(styles.inlineLink)} to={listHref}>Se også udestående arbejde uden for kalenderperioden</Link>.</p>
    </>}
    {view === 'wheel' && <><div {...stylex.props(styles.toolbar)}><Field label="Årshjulets startår"><Input type="number" min={1900} max={9998} value={wheelYear} onChange={event => setParam('wheelYear', event.target.value)} /></Field><Field label="Årshjulets startmåned"><Select value={wheelMonth} onChange={event => setParam('wheelStartMonth', event.target.value)}>{Array.from({ length: 12 }, (_, i) => <option key={i} value={i + 1}>{monthName(`2026-${String(i + 1).padStart(2, '0')}`).split(' ')[0]}</option>)}</Select></Field>
      {(data.companies.some(company => company.canManage) || data.canManageWorkspace) && <Button onClick={() => setSeriesEditor('new')}>Opret rutine</Button>}
      {canCreate && <Button variant="secondary" busy={write.busy} disabled={write.blocked} onClick={() => void materialize()}>Opret aktuelle forekomster</Button>}
    </div><p>Perioder følger hver rutines kalender- eller regnskabsår. Forventede gentagelser er planlægning; de er endnu ikke konkrete opgaver. Frister med ukendt grundlag skal afklares.</p>
      <YearWheel data={data} tasks={tasks} range={range} returnTo={returnTo} />
      <h2>Rutiner</h2>{data.series.length === 0 ? <p>Ingen rutiner oprettet.</p> : <ul {...stylex.props(styles.list)}>{data.series.map(series => <li key={series.seriesId} {...stylex.props(styles.card)}><h3>{series.title}</h3><p>{scopeLabel(series.scope, data)} · {series.active ? 'Aktiv' : 'Pauseret'} · {series.anchor === 'fiscal' ? 'Regnskabsperioder' : 'Kalenderperioder'}</p>{canWriteScope(series.scope, data, true) && <Button variant="secondary" onClick={() => setSeriesEditor(series)}>Redigér eller pausér fremtidige gentagelser</Button>}</li>)}</ul>}
      <p><Link {...stylex.props(styles.inlineLink)} to={listHref}>Se alle udestående forekomster, også fra tidligere år</Link></p>
    </>}
    <details className="card"><summary>Kildedækning og grundlag</summary><p>Opgaver viser arbejdsstatus. Regnskabets korrekthed og overholdelse skal verificeres ved kilden.</p>{data.sourceCoverage.length > 0 ? <ul>{data.sourceCoverage.map(source => <li key={source}>{source}</li>)}</ul> : <p>Kildedækning er ikke verificeret. Brug Opdatér fra kilder for at hente kendt arbejde.</p>}</details>
    {editor && <TaskEditor view={data} initialScope={initialScope} onSaved={state.reload} onClose={() => setEditor(false)} />}
    {seriesEditor && <SeriesEditor view={data} series={seriesEditor === 'new' ? undefined : seriesEditor} initialScope={initialScope} onSaved={state.reload} onClose={() => setSeriesEditor(null)} />}
    {boardEditor && <BoardEditor view={data} board={board} onSaved={state.reload} onClose={() => setBoardEditor(false)} />}
  </section>;
}

function TaskList({ tasks, data, returnTo, onSaved, compact = false }: { tasks: Task[]; data: TasksData; returnTo: string; onSaved: () => void; compact?: boolean }) {
  const { slug } = useParams();
  if (!tasks.length) return <p>Ingen gemte opgaver matcher denne visning.{!compact && ' Kilder opdateres med Opdatér fra kilder; en tom liste beviser ikke, at alt arbejde er afsluttet.'}</p>;
  return <ul {...stylex.props(styles.list)}>{tasks.map(task => <li key={task.taskId} {...stylex.props(styles.card)} data-task-id={task.taskId}>
    <h3 {...stylex.props(styles.title)}><Link to={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`}>{task.title}</Link></h3>
    <p {...stylex.props(styles.meta)}>{scopeLabel(task.scope, data)} · {taskTypeLabel[task.type]} · {task.assignee?.name ?? 'Ikke tildelt'}</p>
    <p {...stylex.props(styles.meta)}>{task.period?.label}{task.workDate ? ` · Arbejdsdato ${task.workDate}` : ''}{task.deadline ? ` · ${task.deadline.kind === 'statutory' ? 'Myndighedsfrist' : 'Frist'} ${task.deadline.date}` : ''}</p>
    <span {...stylex.props(styles.badge)}>{taskUrgency(task)}</span>
    {task.origin === 'proposal' && <><strong>Forslag · skal vurderes</strong>{canWriteScope(task.scope, data) && <AcceptProposal task={task} onSaved={onSaved} />}</>}
    {task.waitingOn && <p>Afventer {task.waitingOn}</p>}
    {task.nextAction && <p>{task.nextAction}</p>}
    {canWriteScope(task.scope, data) && <TaskMove task={task} data={data} aggregate={!slug} onSaved={onSaved} />}
  </li>)}</ul>;
}

function AcceptProposal({ task, onSaved }: { task: Task; onSaved: () => void }) {
  const write = useTaskMutation(onSaved, `accept:${task.taskId}`);
  return <div>{write.feedback}<Button variant="secondary" busy={write.busy} disabled={write.blocked} onClick={() => void write.run(`/api/tasks/${encodeURIComponent(task.taskId)}/update`, { patch: { relevance: 'relevant' }, expectedVersion: task.version })}>Accepter forslag</Button></div>;
}

function TaskMove({ task, data, aggregate, onSaved }: { task: Task; data: TasksData; aggregate: boolean; onSaved: () => void }) {
  const local = boardFor(data, task.scope.kind === 'company' ? task.scope.companySlug : undefined);
  const targetBoard = aggregate ? boardFor(data) : local;
  const [selection, setSelection] = useState('');
  const [localTarget, setLocalTarget] = useState('');
  const [complete, setComplete] = useState(false);
  const [reopen, setReopen] = useState(false);
  const selected = targetBoard.columns.find(column => column.columnId === selection);
  const candidates = selected ? local.columns.filter(column => column.status === selected.status) : [];
  const localDefault = candidates.find(column => column.isDefault);
  const ambiguous = aggregate && task.scope.kind === 'company' && candidates.length > 1 && !localDefault;
  const write = useTaskMutation(onSaved, `move:${task.taskId}`);
  async function move() {
    if (!selected) return;
    if (selected.status === 'done') { setComplete(true); return; }
    if (ambiguous && !localTarget) return;
    await write.run(`/api/tasks/${encodeURIComponent(task.taskId)}/move`, { status: selected.status, expectedVersion: task.version, ...(aggregate ? { workspaceColumnId: selection, ...(task.scope.kind === 'company' ? { columnId: localTarget || localDefault?.columnId || candidates[0]?.columnId } : {}) } : { columnId: selection }) });
    setSelection(''); setLocalTarget('');
  }
  if (task.status === 'done') return <div><Button variant="secondary" onClick={() => setReopen(true)}>Genåbn med årsag</Button>{reopen && <CompletionEditor task={task} reopen onSaved={onSaved} onClose={() => setReopen(false)} />}</div>;
  return <div {...stylex.props(styles.detail)}>{write.feedback}<div {...stylex.props(styles.actions)}>
    <Field label={`Flyt ${task.title}`}><Select value={selection} onChange={event => { setSelection(event.target.value); setLocalTarget(''); }}><option value="">Vælg kolonne</option>{targetBoard.columns.map(column => <option key={column.columnId} value={column.columnId}>{column.name} · {taskStatusLabel[column.status]}</option>)}</Select></Field>
    {ambiguous && <Field label={`Selskabets målkolonne for ${task.title}`}><Select required value={localTarget} onChange={event => setLocalTarget(event.target.value)}><option value="">Vælg selskabets målkolonne</option>{candidates.map(column => <option key={column.columnId} value={column.columnId}>{column.name}</option>)}</Select></Field>}
    <Button variant="secondary" busy={write.busy} disabled={write.blocked || !selection || (ambiguous && !localTarget)} onClick={() => void move()}>Flyt opgave</Button>
  </div>{aggregate && selected && task.scope.kind === 'company' && localDefault && <p className="muted">Selskabets standardvalg: {localDefault.name}</p>}
    {complete && <CompletionEditor task={task} onSaved={onSaved} onClose={() => setComplete(false)} />}
  </div>;
}

function Calendar({ tasks, projections, data, returnTo, range, mode }: { tasks: Task[]; projections: TaskProjection[]; data: TasksData; returnTo: string; range: { from: string; to: string }; mode: string }) {
  const days = dateSequence(range.from, range.to);
  return <div {...stylex.props(mode === 'agenda' ? styles.list : styles.calendar)} role="group" aria-label="Opgavekalender">{days.filter(day => mode !== 'agenda' || tasks.some(task => task.workDate === day || task.deadline?.date === day) || projections.some(projection => !projection.concreteTaskId && (projection.workDate === day || projection.deadline?.date === day))).map(day => <section key={day} {...stylex.props(styles.date)}><h3>{new Intl.DateTimeFormat('da-DK', { weekday: 'short', day: 'numeric', month: 'short', timeZone: 'UTC' }).format(new Date(`${day}T12:00:00Z`))}</h3><ul {...stylex.props(styles.dateList)}>
    {tasks.filter(task => task.workDate === day || task.deadline?.date === day).map(task => <li key={task.taskId}><Link to={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`}>{task.title}</Link><div className="muted">{scopeLabel(task.scope, data)} · {task.workDate === day ? 'Arbejdsdato' : ''}{task.workDate === day && task.deadline?.date === day ? ' + ' : ''}{task.deadline?.date === day ? `${task.deadline.kind === 'statutory' ? 'Myndighedsfrist' : 'Frist'}${task.deadline.certainty === 'unconfirmed' ? ' · skal bekræftes' : ''}` : ''} · {taskUrgency(task)}</div></li>)}
    {projections.filter(projection => !projection.concreteTaskId && (projection.workDate === day || projection.deadline?.date === day)).map(projection => <li key={projection.projectionId}>Forventet: {projection.title}<div className="muted">{scopeLabel(projection.scope, data)} · {projection.period.label} · endnu ikke oprettet{projection.deadline?.certainty === 'unconfirmed' ? ' · frist skal bekræftes' : ''}</div></li>)}
  </ul></section>)}</div>;
}

function YearWheel({ data, tasks, range, returnTo }: { data: TasksData; tasks: Task[]; range: { from: string; to: string }; returnTo: string }) {
  const start = new Date(`${range.from}T12:00:00Z`);
  const months = Array.from({ length: 12 }, (_, i) => { const month = new Date(start); month.setUTCMonth(month.getUTCMonth() + i); return month.toISOString().slice(0, 7); });
  return <div {...stylex.props(styles.wheel)} role="group" aria-label="Årshjul">{months.map(month => <section key={month} {...stylex.props(styles.card)}><h2>{monthName(month)}</h2><ul {...stylex.props(styles.dateList)}>
    {tasks.filter(task => [task.workDate, task.deadline?.date, task.period?.from].some(date => date?.slice(0, 7) === month)).map(task => <li key={task.taskId}><Link to={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`}>{task.title}</Link><p className="muted">{scopeLabel(task.scope, data)} · {task.period?.label} · {taskUrgency(task)}{task.deadline ? ` · frist ${task.deadline.date}` : ''}</p></li>)}
    {data.projections.filter(projection => !projection.concreteTaskId && projection.period.from.slice(0, 7) === month).map(projection => <li key={projection.projectionId}>Forventet: {projection.title}<p className="muted">{scopeLabel(projection.scope, data)} · {projection.period.label} · endnu ikke oprettet{projection.relevance !== 'relevant' ? ' · relevans skal afklares' : ''}{projection.deadline?.certainty === 'unconfirmed' ? ' · frist skal bekræftes' : ''}</p></li>)}
  </ul></section>)}</div>;
}

export function TaskDetailView() {
  const { taskId = '' } = useParams();
  const [params] = useSearchParams();
  const returnTo = taskReturnTo(params.get('returnTo'));
  const state = useAsync(signal => api.task(taskId, { signal }), [taskId]);
  const metadata = useAsync(signal => api.tasks({ includeArchived: true, showDone: true }, { signal }), [taskId]);
  const [editor, setEditor] = useState<'edit' | 'complete' | 'reopen' | 'reminder' | null>(null);
  const reload = () => { state.reload(); metadata.reload(); };
  if ((state.loading || metadata.loading) && (!state.data || !metadata.data)) return <Loading label="Henter opgaven…" />;
  if (!state.data || !metadata.data) return <ErrorState message={state.error ?? metadata.error ?? 'Opgaven er ikke tilgængelig.'} onRetry={reload} />;
  const { task, history } = state.data;
  const data = metadata.data;
  const writable = canWriteScope(task.scope, data);
  return <section {...stylex.props(styles.page)} data-cockpit-page="task-detail">
    <PageHeader title={task.title} description={`${scopeLabel(task.scope, data)} · ${task.taskId} · ${taskTypeLabel[task.type]}`} actions={<><ButtonLink variant="secondary" to={returnTo}>Tilbage til opgaver</ButtonLink>{writable && <Button onClick={() => setEditor('edit')}>Redigér opgave</Button>}</>} />
    {(state.error || metadata.error) && <p role="alert">Senest hentede data vises. Hent aktuel status før ændringer.</p>}
    <div {...stylex.props(styles.detail)}><article {...stylex.props(styles.card)}><h2>{taskUrgency(task)}</h2>
      {task.description && <p>{task.description}</p>}<p>Næste handling: {task.nextAction || 'Ikke angivet'}</p><p>Ansvarlig: {task.assignee?.name ?? 'Ikke tildelt'}{task.assignee?.kind === 'external' ? ' · ekstern, ingen adgang eller besked ved tildeling' : ''}</p>{task.waitingOn && <p>Afventer {task.waitingOn}</p>}
      <p>Periode: {task.period ? `${task.period.label} · ${task.period.from} – ${task.period.to}` : 'Ikke angivet'}</p><p>Arbejdsdato: {task.workDate ?? 'Ikke angivet'}</p>
      {task.deadline ? <><p>{task.deadline.kind === 'statutory' ? 'Myndighedsfrist' : task.deadline.kind === 'agreement' ? 'Aftalt frist' : 'Intern frist'}: {task.deadline.date} · {task.deadline.certainty === 'confirmed' ? 'Bekræftet' : 'Skal bekræftes'}</p><p>Fristgrundlag: {task.deadline.basis || 'Ukendt'}</p></> : <p>Frist: ikke angivet</p>}
      {task.origin === 'proposal' && <><p>Forslag · kræver din vurdering før det accepteres.</p>{writable && <AcceptProposal task={task} onSaved={reload} />}</>}
      <p>{task.evidenceRequired ? 'Afslutningsbevis kræves.' : 'En afslutningsnote er tilstrækkelig.'}</p>
      {task.source && <><h3>Autoritativ kilde</h3><p>{task.source.state === 'unknown' ? 'Kilden skal verificeres' : task.source.state === 'resolved' ? 'Kilden er løst' : 'Kilden er fortsat åben'}</p>{taskSourceHref(task.source.href) && <Link to={`${taskSourceHref(task.source.href)}${taskSourceHref(task.source.href)?.includes('?') ? '&' : '?'}${new URLSearchParams({ returnTo: `/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}` })}`}>Åbn kilde</Link>}</>}
      {task.references.length > 0 && <><h3>Referencer</h3><References references={task.references} scope={task.scope} returnTo={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`} /></>}
      {writable && <><TaskMove task={task} data={data} aggregate={task.scope.kind !== 'company'} onSaved={reload} /><div {...stylex.props(styles.actions)}><Button onClick={() => setEditor(task.status === 'done' ? 'reopen' : 'complete')}>{task.status === 'done' ? 'Genåbn opgave' : 'Afslut med dokumentation'}</Button><Button variant="secondary" onClick={() => setEditor('reminder')}>Påmindelser</Button></div></>}
    </article>
    <article {...stylex.props(styles.card)}><h2>Afslutning og påmindelser</h2>{task.completion ? <><p>{outcomeLabel[task.completion.outcome]} · {task.completion.assurance === 'product_verified' ? 'Verificeret ved produktkilden' : 'Oplyst af brugeren'}</p><p>{task.completion.note}</p><p>{task.completion.at} · {task.completion.actor}</p><References references={task.completion.references} scope={task.scope} returnTo={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`} /></> : <p>Ingen registreret afslutning.</p>}
      {task.reminders.length === 0 ? <p>Ingen aktiverede påmindelser.</p> : <ul>{task.reminders.map(reminder => <li key={reminder.reminderId}>{reminder.enabled && data.runtime.running && task.status !== 'done' ? 'Aktiv levering' : 'Inaktiv levering'} · {reminder.kind === 'deadline' ? `${reminder.daysBefore ?? 0} dage før frist ${task.deadline?.date ?? '(fristen mangler)'}${task.deadline ? ` · planlagt fra ${dateAdd(task.deadline.date, -(reminder.daysBefore ?? 0))}` : ''}` : `Opfølgning ${reminder.followUpDate}`} · {reminder.frequency === 'daily' ? 'dagligt' : 'én gang'} · {reminder.timeZone} · Rentemester</li>)}</ul>}
      {!data.runtime.running && <p>Påmindelsesruntime er stoppet. Gemte planer leverer ikke beskeder.</p>}
    </article>
    <article {...stylex.props(styles.card)}><h2>Historik</h2>{history.length ? <ol>{history.map(event => <li key={`${event.version}:${event.operation}`}><strong>{event.operation}</strong> · {event.at} · {event.actor} · version {event.version}<p>{taskStatusLabel[event.task.status]} · {event.task.assignee?.name ?? 'Ikke tildelt'} · arbejdsdato {event.task.workDate ?? 'ingen'} · frist {event.task.deadline?.date ?? 'ingen'}</p>{event.task.completion && <><p>{outcomeLabel[event.task.completion.outcome]}: {event.task.completion.note}</p><References references={event.task.completion.references} scope={event.task.scope} returnTo={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`} /></>}</li>)}</ol> : <p>Ingen historik tilgængelig.</p>}</article></div>
    {editor === 'edit' && <TaskEditor task={task} view={data} onSaved={reload} onClose={() => setEditor(null)} />}
    {(editor === 'complete' || editor === 'reopen') && <CompletionEditor task={task} reopen={editor === 'reopen'} onSaved={reload} onClose={() => setEditor(null)} />}
    {editor === 'reminder' && <ReminderEditor task={task} view={data} onSaved={reload} onClose={() => setEditor(null)} />}
  </section>;
}
function References({ references, scope, returnTo }: { references: Task['references']; scope: TaskScope; returnTo: string }) { return <ul>{references.map((reference, i) => { const href = referenceHref(reference, scope); return <li key={`${reference.kind}:${reference.ref}:${i}`}>{href ? <Link to={`${href}${href.includes('?') ? '&' : '?'}${new URLSearchParams({ returnTo })}`}>{reference.kind}: {reference.ref}</Link> : <span>{reference.kind}: {reference.ref}</span>}</li>; })}</ul>; }
