import { afterEach, describe, expect, test, vi } from 'bun:test';
import { cleanup, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TasksView, TaskDetailView } from './TasksView';
import { SeriesEditor, TaskEditor } from './TaskForms';
import type { TaskSeries } from '../lib/tasks';
import { renderAt } from '../test/render';
import { mockFetch } from '../test/fixtures';
import { syntheticTask, syntheticTasksView } from '../test/fixtures/tasks';
import { defaultColumns, parseReferences, referenceHref, taskReturnTo, taskSourceHref } from '../lib/tasks';

const renderTasks = (route = '/opgaver') => renderAt(<TasksView />, { route, path: route.startsWith('/companies') ? '/companies/:slug/opgaver' : '/opgaver' });
function writes() { return (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([, init]) => init?.method === 'POST'); }
afterEach(cleanup);
describe('tasks workspace', () => {
  test('one task links to its stable detail and ignores inherited accounting year', async () => {
    mockFetch({ 'GET /api/tasks': syntheticTasksView() }); renderTasks('/companies/acme-aps/opgaver?year=2024');
    const task = await screen.findByRole('link', { name: 'Afstem oktober' });
    expect(task.getAttribute('href')).toContain('/opgaver/task-month?returnTo=');
    expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([url]) => String(url).includes('/api/tasks')).every(([url]) => !String(url).includes('year='))).toBe(true);
    expect(screen.getByText(/Arbejdsdato 2026-10-09.*Myndighedsfrist 2026-10-05/)).toBeInTheDocument();
    expect(writes()).toHaveLength(0);
  });
  test('creation requires title and an explicit aggregate scope', async () => {
    mockFetch({ 'GET /api/tasks': syntheticTasksView(), 'POST /api/tasks': { task: syntheticTask({ taskId: 'created', title: 'Ny aftale' }) } }); renderTasks();
    await userEvent.click(await screen.findByRole('button', { name: 'Opret opgave' }));
    const form = screen.getByRole('dialog', { name: 'Opret opgave' });
    await userEvent.type(within(form).getByLabelText('Titel', { exact: true }), 'Ny aftale');
    expect(within(form).getByRole('button', { name: 'Gem opgave' })).toBeDisabled();
    await userEvent.selectOptions(within(form).getByLabelText('Opgavens selskab eller fælles scope'), 'beta-aps');
    await userEvent.click(within(form).getByRole('button', { name: 'Gem opgave' }));
    await screen.findByRole('button', { name: 'Opret opgave' });
    expect(writes()).toHaveLength(1);
    expect(JSON.parse(String((writes()[0]![1] as RequestInit).body))).toMatchObject({ title: 'Ny aftale', scope: { kind: 'company', companySlug: 'beta-aps' }, confirm: true, idempotencyKey: expect.any(String) });
  });
  test('all views show common task IDs and keyboard move asks ambiguous local column', async () => {
    const data = syntheticTasksView();
    data.boards[1]!.columns = [...defaultColumns().filter(column => column.status !== 'waiting'), { columnId: 'accountant', name: 'Hos revisor', status: 'waiting', isDefault: false }, { columnId: 'approval', name: 'Klar til godkendelse', status: 'waiting', isDefault: false }];
    mockFetch({ 'GET /api/tasks': data, 'POST /api/tasks/task-month/move': { task: syntheticTask({ status: 'waiting', columnId: 'accountant' }) } }); renderTasks('/opgaver?view=kanban');
    await screen.findByRole('heading', { name: 'Kanban' });
    const card = screen.getByRole('link', { name: 'Afstem oktober' }).closest('li')!;
    await userEvent.selectOptions(within(card).getByLabelText('Flyt Afstem oktober'), 'waiting');
    const button = within(card).getByRole('button', { name: 'Flyt opgave' });
    expect(button).toBeDisabled();
    await userEvent.selectOptions(within(card).getByLabelText('Selskabets målkolonne for Afstem oktober'), 'accountant');
    await userEvent.click(button);
    expect(JSON.parse(String((writes()[0]![1] as RequestInit).body))).toMatchObject({ status: 'waiting', columnId: 'accountant', workspaceColumnId: 'waiting', expectedVersion: 1 });
    await userEvent.click(screen.getByRole('button', { name: 'Kalender' }));
    await screen.findByLabelText('Kalendervisning');
    await userEvent.type(screen.getByLabelText('Kalenderdato'), '2026-10-08');
    await userEvent.click(screen.getByRole('button', { name: 'Årshjul' }));
    await screen.findByLabelText('Årshjulets startår');
  });
  test('evidence requirement is not bypassed by a done column', async () => {
    mockFetch({ 'GET /api/tasks': syntheticTasksView() }); renderTasks();
    const card = (await screen.findByRole('link', { name: 'Indberet periodens moms' })).closest('li')!;
    await userEvent.selectOptions(within(card).getByLabelText('Flyt Indberet periodens moms'), 'done');
    await userEvent.click(within(card).getByRole('button', { name: 'Flyt opgave' }));
    const dialog = screen.getByRole('dialog', { name: 'Afslut opgave' });
    await userEvent.type(within(dialog).getByLabelText('Afslutningsnote eller begrundelse'), 'Indberetning vurderet');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Gem afslutning' }));
    expect(await within(dialog).findByText(/Denne opgave kræver et afslutningsbevis/)).toBeInTheDocument();
    expect(writes()).toHaveLength(0);
  });
  test('read-only scope hides per-card writes even on aggregate route', async () => {
    const data = syntheticTasksView(); data.companies[1]!.canWrite = false;
    mockFetch({ 'GET /api/tasks': data }); renderTasks();
    const card = (await screen.findByRole('link', { name: 'Indberet periodens moms' })).closest('li')!;
    expect(within(card).queryByRole('button', { name: 'Flyt opgave' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('Flyt Afstem oktober')).toBeInTheDocument();
  });
  test('detail retains documentary history and shows inactive runtime honestly', async () => {
    const task = syntheticTask({ status: 'done', reminders: [{ reminderId: 'r1', recipientId: 'synthetic-owner', enabled: true, kind: 'deadline', daysBefore: 7, frequency: 'once', timeZone: 'Europe/Copenhagen' }], completion: { outcome: 'exception', note: 'Begrundet testundtagelse', references: [], assurance: 'user_reported', at: '2026-10-08T08:00:00Z', actor: 'user:synthetic' } });
    const data = syntheticTasksView({ tasks: [task], runtime: { running: false, lastTickAt: null, lastError: null } });
    mockFetch({ 'GET /api/tasks': data, 'GET /api/tasks/task-month': { task, history: [{ taskId: task.taskId, version: 1, operation: 'complete', task, actor: 'user:synthetic', principal: 'synthetic-owner', at: '2026-10-08T08:00:00Z' }] } });
    renderAt(<TaskDetailView />, { route: '/opgaver/task-month?returnTo=https://evil.invalid', path: '/opgaver/:taskId' });
    expect(await screen.findByRole('link', { name: 'Tilbage til opgaver' })).toHaveAttribute('href', '/opgaver');
    expect(screen.getByRole('heading', { name: 'Historik' })).toBeInTheDocument();
    expect(screen.getByText(/Påmindelsesruntime er stoppet/)).toBeInTheDocument();
    expect(screen.getByText(/Oplyst af brugeren/)).toBeInTheDocument();
  });
});
test('source links and return paths reject foreign origins; shared references keep explicit company', () => {
  expect(taskReturnTo('//evil.invalid/opgaver')).toBe('/opgaver');
  expect(taskReturnTo('/companies/acme-aps/opgaver?view=kanban')).toBe('/companies/acme-aps/opgaver?view=kanban');
  expect(taskSourceHref('javascript:alert(1)')).toBeNull();
  expect(parseReferences('[beta-aps]document:12\nexternal_receipt:ABC')).toEqual([{ kind: 'document', ref: '12', companySlug: 'beta-aps' }, { kind: 'external_receipt', ref: 'ABC' }]);
});

test('a version conflict refreshes the current task while preserving edited fields for explicit reconciliation', async () => {
  let task = syntheticTask();
  globalThis.fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = String(input).split('?')[0];
    if (path === '/api/tasks/task-month/update') {
      task = syntheticTask({ version: 2, title: 'Ændret af en anden bruger', nextAction: 'Andens handling' });
      return Response.json({ ok: false, code: 'conflict', errors: ['Versionen er ændret'] }, { status: 409 });
    }
    if (path === '/api/tasks/task-month') return Response.json({ ok: true, task, history: [] });
    if (path === '/api/tasks') return Response.json({ ok: true, ...syntheticTasksView({ tasks: [task] }) });
    throw new Error(`Unexpected test endpoint ${path} ${init?.method ?? 'GET'}`);
  }) as unknown as typeof fetch;
  renderAt(<TaskDetailView />, { route: '/opgaver/task-month', path: '/opgaver/:taskId' });
  await userEvent.click(await screen.findByRole('button', { name: 'Redigér opgave' }));
  const form = screen.getByRole('dialog', { name: 'Redigér opgave' });
  await userEvent.clear(within(form).getByLabelText('Titel'));
  await userEvent.type(within(form).getByLabelText('Titel'), 'Min bevarede ændring');
  await userEvent.click(within(form).getByRole('button', { name: 'Gem opgave' }));
  await within(form).findByText(/Opgaven har nu version 2/);
  expect(within(form).getByLabelText('Titel')).toHaveValue('Min bevarede ændring');
  expect(within(form).getByRole('button', { name: 'Gem opgave' })).toBeDisabled();
  await userEvent.click(within(form).getByRole('button', { name: 'Brug den aktuelle version som grundlag' }));
  expect(within(form).getByRole('button', { name: 'Gem opgave' })).toBeEnabled();
});

test('column removal requires relocation and meaning changes require a visible preview before application', async () => {
  mockFetch({ 'GET /api/tasks': syntheticTasksView(), 'POST /api/task-boards/preview': { preview: { previewHash: 'board-preview', affectedTaskIds: ['task-month'], statusChanges: [{ taskId: 'task-month', from: 'open', to: 'waiting' }] } }, 'POST /api/task-boards': { board: syntheticTasksView().boards[0] } });
  renderTasks('/opgaver?view=kanban');
  await userEvent.click(await screen.findByRole('button', { name: 'Tilpas kolonner' }));
  const form = screen.getByRole('dialog', { name: 'Tilpas kolonner' });
  await userEvent.click(within(form).getByRole('button', { name: 'Fjern Åben' }));
  expect(within(form).getByRole('button', { name: 'Vis konsekvenser' })).toBeDisabled();
  await userEvent.selectOptions(within(form).getByLabelText('Flyt opgaver fra Åben til'), 'waiting');
  await userEvent.click(within(form).getByRole('button', { name: 'Vis konsekvenser' }));
  expect(await within(form).findByText(/1 opgaver skifter fælles status/)).toBeInTheDocument();
  expect(writes().filter(([url]) => String(url) === '/api/task-boards')).toHaveLength(0);
  await userEvent.click(within(form).getByRole('button', { name: 'Anvend viste ændringer' }));
  const body = JSON.parse(String((writes().find(([url]) => String(url) === '/api/task-boards')![1] as RequestInit).body));
  expect(body).toMatchObject({ expectedVersion: 0, board: { previewHash: 'board-preview', relocations: { open: 'waiting' } } });
});

test('a company reviewer can create a linked shared task but cannot create an empty workspace task', async () => {
  const data = syntheticTasksView({ canManageWorkspace: false });
  data.companies.forEach(company => { company.canManage = false; });
  mockFetch({ 'GET /api/tasks': data, 'POST /api/tasks': { task: syntheticTask() } });
  renderAt(<TaskEditor view={data} onSaved={() => {}} onClose={() => {}} />);
  const form = screen.getByRole('dialog', { name: 'Opret opgave' });
  await userEvent.type(within(form).getByLabelText('Titel'), 'Fælles kontrol');
  await userEvent.selectOptions(within(form).getByLabelText('Opgavens selskab eller fælles scope'), '@workspace');
  expect(within(form).getByRole('button', { name: 'Gem opgave' })).toBeDisabled();
  expect(within(form).getByText(/Vælg mindst ét selskab/)).toBeInTheDocument();
  await userEvent.click(within(form).getByLabelText('Acme ApS'));
  await userEvent.click(within(form).getByLabelText('Beta ApS'));
  await userEvent.click(within(form).getByRole('button', { name: 'Gem opgave' }));
  expect(JSON.parse(String((writes()[0]![1] as RequestInit).body))).toMatchObject({ scope: { kind: 'workspace', companySlugs: ['acme-aps', 'beta-aps'] } });
});

test('series editing sends a public draft and preserves existing evidence requirements', async () => {
  const template = syntheticTask({ evidenceRequired: true, references: [{ kind: 'document', ref: '12', companySlug: 'acme-aps' }] });
  const series: TaskSeries = { seriesId: 'series-safe', version: 2, title: 'Afstem hver måned', scope: template.scope, template, cadence: 'month', every: 1, anchor: 'calendar', startDate: '2026-01-01', endDate: null, fiscalYearStartMonth: 1, workDayOffset: 4, deadlineDayOffset: null, relevance: 'relevant', active: true, updatedAt: '2026-10-08T08:00:00Z' };
  mockFetch({ 'POST /api/task-series': { series } });
  renderAt(<SeriesEditor view={syntheticTasksView()} series={series} onSaved={() => {}} onClose={() => {}} />);
  const form = screen.getByRole('dialog', { name: 'Redigér fremtidige gentagelser' });
  expect(within(form).getByLabelText('Kræv afslutningsbevis')).toBeDisabled();
  await userEvent.clear(within(form).getByLabelText('Næste handling'));
  await userEvent.type(within(form).getByLabelText('Næste handling'), 'Ny handling for fremtiden');
  await userEvent.click(within(form).getByRole('button', { name: 'Gem rutine' }));
  const body = JSON.parse(String((writes()[0]![1] as RequestInit).body));
  expect(body).toMatchObject({ expectedVersion: 2, series: { template: { nextAction: 'Ny handling for fremtiden', evidenceRequired: true, references: template.references } } });
  expect(Object.keys(body.series.template).sort()).toEqual(['title', 'scope', 'type', 'description', 'assignee', 'waitingOn', 'workDate', 'period', 'references', 'relevance', 'verificationRequired', 'nextAction', 'evidenceRequired', 'deadline'].sort());
});

test('materializing a filtered year wheel sends only the selected companies', async () => {
  mockFetch({ 'GET /api/tasks': syntheticTasksView(), 'POST /api/task-series/materialize': { tasks: [] } });
  renderTasks('/opgaver?view=wheel&companySlug=beta-aps');
  await userEvent.click(await screen.findByRole('button', { name: 'Opret aktuelle forekomster' }));
  expect(JSON.parse(String((writes()[0]![1] as RequestInit).body))).toMatchObject({ companySlugs: ['beta-aps'] });
});

test('an unfiltered year wheel leaves company scope unspecified', async () => {
  mockFetch({ 'GET /api/tasks': syntheticTasksView(), 'POST /api/task-series/materialize': { tasks: [] } });
  renderTasks('/opgaver?view=wheel');
  await userEvent.click(await screen.findByRole('button', { name: 'Opret aktuelle forekomster' }));
  expect(JSON.parse(String((writes()[0]![1] as RequestInit).body))).not.toHaveProperty('companySlugs');
});

test('an unclear external result requires evidence, relevance and a documented verification before clearing the flag', async () => {
  const task = syntheticTask({ verificationRequired: true, evidenceRequired: true, relevance: 'unknown' });
  mockFetch({ 'GET /api/tasks': syntheticTasksView(), 'POST /api/tasks/task-month/update': { task } });
  renderAt(<TaskEditor view={syntheticTasksView()} task={task} onSaved={() => {}} onClose={() => {}} />);
  const form = screen.getByRole('dialog', { name: 'Redigér opgave' });
  expect(within(form).getByLabelText('Kræv afslutningsbevis')).toBeChecked();
  expect(within(form).getByLabelText('Kræv afslutningsbevis')).toBeDisabled();
  await userEvent.click(within(form).getByLabelText('Resultat skal verificeres før nyt forsøg'));
  await userEvent.type(within(form).getByLabelText('Hvad har du verificeret?'), 'Kvitteringen bekræfter indberetningen.');
  await userEvent.click(within(form).getByRole('button', { name: 'Gem opgave' }));
  expect(await within(form).findByRole('alert')).toHaveTextContent(/vælg Relevant, tilføj et bevis/);
  expect(writes()).toHaveLength(0);
  await userEvent.selectOptions(within(form).getByLabelText('Relevans', { exact: true }), 'relevant');
  await userEvent.type(within(form).getByLabelText('Sikre referencer'), 'external_receipt:SYNTHETIC-12');
  await userEvent.click(within(form).getByRole('button', { name: 'Gem opgave' }));
  expect(JSON.parse(String((writes()[0]![1] as RequestInit).body))).toMatchObject({ patch: { verificationRequired: false, evidenceRequired: true, relevance: 'relevant', references: [{ kind: 'external_receipt', ref: 'SYNTHETIC-12', companySlug: 'acme-aps' }], description: expect.stringContaining('Verifikation: Kvitteringen bekræfter indberetningen.') } });
});

test('reference navigation distinguishes protected document, party, knowledge and batch approval pages', () => {
  const scope = { kind: 'company' as const, companySlug: 'acme-aps' };
  expect(referenceHref({ kind: 'document', ref: '12' }, scope)).toBe('/companies/acme-aps/bilag/12');
  expect(referenceHref({ kind: 'party', ref: 'party-synthetic' }, scope)).toBe('/companies/acme-aps/parter/party-synthetic');
  expect(referenceHref({ kind: 'knowledge', ref: 'synthetic-playbook' }, scope)).toBe('/companies/acme-aps/viden/synthetic-playbook');
  expect(referenceHref({ kind: 'knowledge', ref: 'synthetic-playbook' }, { kind: 'workspace', companySlugs: [] })).toBe('/viden/synthetic-playbook');
  expect(referenceHref({ kind: 'approval', ref: 'bookkeeping-batch:7:9' }, scope)).toBe('/companies/acme-aps/batchbogfoering?runId=7');
  expect(referenceHref({ kind: 'approval', ref: 'synthetic-draft' }, scope)).toBe('/companies/acme-aps/kladder');
});

test('a failed first source sync preserves uncertainty even when the task list is empty', async () => {
  mockFetch({ 'GET /api/tasks': syntheticTasksView({ tasks: [], count: 0 }), 'POST /api/tasks/sync': { scope: { companySlugs: ['acme-aps'], includesWorkspace: false }, sync: { created: 0, updated: 0, resolved: 0, reopened: 0, unknown: 0, errors: [{ companySlug: 'acme-aps', reason: 'Syntetisk kilde kunne ikke åbnes' }] } } });
  renderTasks();
  await userEvent.click(await screen.findByRole('button', { name: 'Opdatér fra kilder' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Kildegrundlaget er ufuldstændigt');
  expect(screen.getByRole('alert')).toHaveTextContent('Acme ApS: Syntetisk kilde kunne ikke åbnes');
  expect(screen.getByRole('alert')).toHaveTextContent('En tom liste betyder ikke, at arbejdet er afsluttet.');
});
