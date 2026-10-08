import { expect, test, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { mockApi, type MockResponse } from './fixtures';
import { syntheticTask, syntheticTasksView } from '../src/test/fixtures/tasks';
import type { Task, TaskBoardDraft, TaskSeriesDraft, TasksView } from '../src/lib/tasks';

async function taskFixture(page: Page, options: { interruptedCreate?: boolean; readOnly?: boolean } = {}) {
  const state = syntheticTasksView();
  if (options.readOnly) { state.canManageWorkspace = false; state.companies = state.companies.map(company => ({ ...company, canWrite: false, canManage: false })); }
  const history = new Map<string, Array<{ taskId: string; task: Task; version: number; operation: string; actor: string; principal: string; at: string }>>();
  const receipts = new Map<string, { operation: string; taskId?: string; version?: number }>();
  const overrides: Parameters<typeof mockApi>[1] = { overrides: {
    'GET /api/tasks': async route => {
      const params = new URL(route.request().url()).searchParams;
      const companies = params.getAll('companySlug');
      const tasks = state.tasks.filter(task => (!companies.length || (task.scope.kind === 'company' ? companies.includes(task.scope.companySlug) : task.scope.companySlugs.some(slug => companies.includes(slug)))) && (params.get('showDone') === 'true' || task.status !== 'done') && (!params.get('search') || task.title.includes(params.get('search')!)));
      const response: TasksView = { ...state, tasks, count: tasks.length };
      return { body: { ok: true, ...response } };
    },
    'POST /api/tasks': async route => {
      const body = route.request().postDataJSON();
      const task = syntheticTask({ ...body, taskId: 'task-created', title: body.title });
      state.tasks.push(task); record(task, 'create'); receipts.set(body.idempotencyKey, { operation: 'create', taskId: task.taskId, version: 1 });
      if (options.interruptedCreate) return { status: 500, body: { ok: false, code: 'internal', errors: ['Syntetisk afbrudt svar efter commit'] } };
      return { body: { ok: true, task } };
    },
    'POST /api/task-boards/preview': async route => {
      const board = route.request().postDataJSON().board as TaskBoardDraft;
      const changes = state.tasks.flatMap(task => { const column = board.columns.find(column => column.columnId === (task.workspaceColumnId ?? task.status)); return column && column.status !== task.status ? [{ taskId: task.taskId, from: task.status, to: column.status }] : []; });
      return { body: { ok: true, preview: { previewHash: 'synthetic-preview', affectedTaskIds: changes.map(change => change.taskId), statusChanges: changes } } };
    },
    'POST /api/task-boards': async route => {
      const body = route.request().postDataJSON(); const board = body.board as TaskBoardDraft;
      const current = state.boards.find(item => item.boardId === board.boardId)!;
      Object.assign(current, { ...board, version: current.version + 1 });
      return { body: { ok: true, board: current } };
    },
    'POST /api/task-series': async route => {
      const draft = route.request().postDataJSON().series as TaskSeriesDraft;
      const series = { ...draft, version: 1, updatedAt: '2026-10-08T08:00:00Z' };
      state.series.push(series); return { body: { ok: true, series } };
    },
  } };
  function record(task: Task, operation: string) { history.set(task.taskId, [...(history.get(task.taskId) ?? []), { taskId: task.taskId, task: structuredClone(task), operation, version: task.version, actor: 'user:synthetic-owner', principal: 'synthetic-owner', at: '2026-10-08T08:00:00Z' }]); }
  // The shared fixture still fails closed on unrelated or unplanned writes.
  const fixture = await mockApi(page, overrides);
  await page.route('**/api/tasks/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path.startsWith('/api/tasks/operations/')) { const key = decodeURIComponent(path.split('/').at(-1)!); await route.fulfill({ json: { ok: true, receipt: receipts.get(key) ?? null } }); return; }
    const parts = path.split('/'); const task = state.tasks.find(task => task.taskId === parts[3]);
    if (!task) { await route.fulfill({ status: 404, json: { ok: false, code: 'not_found', errors: ['Syntetisk opgave findes ikke'] } }); return; }
    let response: MockResponse;
    if (route.request().method() === 'GET') response = { body: { ok: true, task, history: history.get(task.taskId) ?? [] } };
    else {
      const body = route.request().postDataJSON(); const action = parts[4];
      if (body.expectedVersion !== task.version) { await route.fulfill({ status: 409, json: { ok: false, code: 'conflict', errors: ['Opgaven er ændret af en anden bruger'] } }); return; }
      if (action === 'update') Object.assign(task, body.patch);
      else if (action === 'move') Object.assign(task, { status: body.status, columnId: body.columnId ?? task.columnId, workspaceColumnId: body.workspaceColumnId ?? task.workspaceColumnId });
      else if (action === 'complete') Object.assign(task, { status: 'done', columnId: 'done', workspaceColumnId: 'done', completion: { outcome: body.outcome, note: body.note, references: body.references ?? [], assurance: 'user_reported', at: '2026-10-08T08:00:00Z', actor: 'user:synthetic-owner' } });
      else if (action === 'reopen') Object.assign(task, { status: 'open', columnId: 'open', workspaceColumnId: 'open' });
      else if (action === 'reminder') task.reminders = [body.reminder];
      else { await route.fulfill({ status: 503, json: { ok: false, code: 'fixture_required', errors: ['Uventet syntetisk mutation'] } }); return; }
      task.version++; record(task, action); receipts.set(body.idempotencyKey, { operation: action, taskId: task.taskId, version: task.version });
      response = { body: { ok: true, task } };
    }
    await route.fulfill({ status: response.status ?? 200, json: response.body });
  });
  await page.clock.install({ time: new Date('2026-10-08T08:00:00Z') });
  return { state, fixture };
}

test('tasks share scope, planning, columns, evidence and reminders across every view', async ({ page }) => {
  const { fixture } = await taskFixture(page);
  await page.goto('/opgaver');
  await expect(page.getByRole('heading', { name: 'Opgaver', exact: true })).toBeVisible();
  await expect(page.getByText('Forfaldent', { exact: true })).toHaveCount(2);
  await page.getByRole('button', { name: 'Opret opgave', exact: true }).click();
  const create = page.getByRole('dialog', { name: 'Opret opgave' });
  await create.getByLabel('Titel', { exact: true }).fill('Afklar syntetisk kontrakt');
  await expect(create.getByRole('button', { name: 'Gem opgave' })).toBeDisabled();
  await create.getByLabel('Opgavens selskab eller fælles scope').selectOption('beta-aps');
  await create.locator('summary').click();
  await create.getByLabel('Arbejdsdato', { exact: true }).fill('2026-10-10');
  await create.getByLabel('Frist', { exact: true }).fill('2026-10-20');
  await create.getByLabel('Grundlag for fristen').fill('Syntetisk aftale, bilag 1');
  await create.getByLabel('Fristens art').selectOption('agreement');
  await create.getByLabel('Fristens sikkerhed').selectOption('confirmed');
  await create.getByLabel('Kræv afslutningsbevis').check();
  await create.getByRole('button', { name: 'Gem opgave' }).click();
  await expect(create).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Afklar syntetisk kontrakt' })).toBeVisible();
  await page.getByRole('button', { name: 'Kalender', exact: true }).click();
  await page.getByLabel('Kalendervisning').selectOption('agenda');
  await expect(page.getByRole('link', { name: 'Afklar syntetisk kontrakt' })).toHaveCount(2);
  await expect(page.getByRole('main')).toContainText('Arbejdsdato');
  await page.getByRole('button', { name: 'Kanban', exact: true }).click();
  await page.getByRole('button', { name: 'Tilpas kolonner' }).click();
  const board = page.getByRole('dialog', { name: 'Tilpas kolonner' });
  await board.getByRole('button', { name: 'Tilføj kolonne' }).click();
  await board.getByLabel('Kolonnenavn 5').fill('Hos revisor');
  await board.getByRole('button', { name: 'Vis konsekvenser' }).click();
  await board.getByRole('button', { name: 'Anvend viste ændringer' }).click();
  await expect(board).toHaveCount(0);
  const card = page.locator('[data-task-id="task-created"]');
  await card.getByLabel('Flyt Afklar syntetisk kontrakt').selectOption({ label: 'Hos revisor · Afventer' });
  await card.getByRole('button', { name: 'Flyt opgave' }).click();
  await expect(page.getByRole('region', { name: 'Kolonne Hos revisor' })).toContainText('Afklar syntetisk kontrakt');
  await page.getByRole('button', { name: 'Årshjul', exact: true }).click();
  await expect(page.getByLabel('Årshjul', { exact: true })).toContainText('Forventet: Afstem november');
  await page.getByRole('button', { name: 'Opret rutine' }).click();
  const series = page.getByRole('dialog', { name: 'Opret rutine' });
  await series.getByLabel('Rutinens titel').fill('Månedlig dokumentkontrol');
  await series.getByLabel('Opgavens selskab eller fælles scope').selectOption('acme-aps');
  await series.getByRole('button', { name: 'Gem rutine' }).click();
  await expect(series).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Månedlig dokumentkontrol' })).toBeVisible();
  await page.getByRole('button', { name: 'Liste', exact: true }).click();
  await page.getByRole('link', { name: 'Afklar syntetisk kontrakt' }).click();
  await page.getByRole('button', { name: 'Påmindelser', exact: true }).click();
  const reminder = page.getByRole('dialog', { name: 'Påmindelser i Rentemester' });
  await expect(reminder.getByLabel('Aktivér denne påmindelsesplan')).not.toBeChecked();
  await reminder.getByLabel('Aktivér denne påmindelsesplan').check();
  await reminder.getByRole('button', { name: 'Gem påmindelsesplan' }).click();
  await expect(page.getByRole('main')).toContainText('Aktiv levering');
  await page.getByRole('button', { name: 'Afslut med dokumentation' }).click();
  const complete = page.getByRole('dialog', { name: 'Afslut opgave' });
  await complete.getByLabel('Afslutningsnote eller begrundelse').fill('Bekræftet ved syntetisk modtagelse');
  await complete.getByRole('button', { name: 'Gem afslutning' }).click();
  await expect(complete).toContainText('Denne opgave kræver et afslutningsbevis');
  await complete.getByLabel('Sikre referencer').fill('external_receipt:SYNTHETIC-RECEIPT');
  await complete.getByRole('button', { name: 'Gem afslutning' }).click();
  await expect(page.getByRole('main')).toContainText('Oplyst af brugeren');
  await expect(page.getByRole('main')).toContainText('SYNTHETIC-RECEIPT');
  await expect(page.getByRole('main')).toContainText('Inaktiv levering');
  await page.getByRole('button', { name: 'Genåbn opgave', exact: true }).click();
  const reopen = page.getByRole('dialog', { name: 'Genåbn opgave' });
  await reopen.getByLabel('Årsag til genåbning').fill('Ny syntetisk afklaring');
  await reopen.getByRole('button', { name: 'Genåbn med årsag' }).click();
  await expect(page.getByRole('heading', { name: 'Historik', exact: true })).toBeVisible();
  await expect(page.getByRole('main')).toContainText('SYNTHETIC-RECEIPT');
  await page.getByRole('link', { name: 'Tilbage til opgaver' }).click();
  await expect(page).toHaveURL(/\/opgaver\?view=list/);
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
  expect(results.violations.map(({ id, nodes }) => ({ id, targets: nodes.map(node => node.target) }))).toEqual([]);
  fixture.assertComplete();
});

test('mobile task views fit 320px and keyboard status selection needs no dragging', async ({ page, browserName }) => {
  await page.setViewportSize({ width: 320, height: 844 });
  const { fixture } = await taskFixture(page);
  await page.goto('/opgaver?view=calendar');
  await expect(page.getByLabel('Kalendervisning')).toHaveValue('agenda');
  await page.getByRole('button', { name: 'Kanban', exact: true }).click();
  const card = page.locator('[data-task-id="task-month"]');
  const select = card.getByLabel('Flyt Afstem oktober');
  await select.focus();
  await select.selectOption('in_progress');
  if (browserName === 'webkit') await card.getByRole('button', { name: 'Flyt opgave' }).focus();
  else await page.keyboard.press('Tab');
  await expect(card.getByRole('button', { name: 'Flyt opgave' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('region', { name: 'Kolonne I gang' })).toContainText('Afstem oktober');
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
  expect(results.violations.map(({ id, nodes }) => ({ id, targets: nodes.map(node => node.target) }))).toEqual([]);
  fixture.assertComplete();
});

test('unknown creation verifies the same opaque key with a read after reload', async ({ page }) => {
  const { fixture, state } = await taskFixture(page, { interruptedCreate: true });
  await page.goto('/opgaver');
  await page.getByRole('button', { name: 'Opret opgave', exact: true }).click();
  let form = page.getByRole('dialog', { name: 'Opret opgave' });
  await form.getByLabel('Titel', { exact: true }).fill('Syntetisk ukendt resultat');
  await form.getByLabel('Opgavens selskab eller fælles scope').selectOption('acme-aps');
  await form.getByRole('button', { name: 'Gem opgave' }).click();
  await expect(form).toContainText('Serverens resultat kunne ikke bekræftes');
  await expect(form.getByRole('button', { name: 'Gem opgave' })).toBeDisabled();
  expect(await page.evaluate(() => Object.values(sessionStorage).some(value => value.includes('Syntetisk ukendt resultat')))).toBe(false);
  await page.reload();
  await page.getByRole('button', { name: 'Opret opgave', exact: true }).click();
  form = page.getByRole('dialog', { name: 'Opret opgave' });
  await expect(form.getByRole('button', { name: 'Gem opgave' })).toBeDisabled();
  await form.getByRole('button', { name: 'Verificér samme handling' }).click();
  await expect(form).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Syntetisk ukendt resultat' })).toHaveCount(1);
  expect(state.tasks.filter(task => task.title === 'Syntetisk ukendt resultat')).toHaveLength(1);
  expect(fixture.calls.filter(call => call.method === 'POST' && call.path === '/api/tasks')).toHaveLength(1);
  fixture.assertComplete();
});
