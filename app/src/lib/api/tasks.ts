import { request, type ReadRequestOptions } from './_shared';
import type { Task, TaskBoard, TaskBoardDraft, TaskBoardPreview, TaskEvent, TaskQuery, TasksView } from '../../../../src/core/tasks-types';

export const tasksApi = {
  tasks(query: TaskQuery = {}, options?: ReadRequestOptions) {
    const params = new URLSearchParams();
    for (const slug of query.companySlugs ?? []) params.append('companySlug', slug);
    for (const [key, value] of Object.entries(query)) if (key !== 'companySlugs' && value !== undefined) params.set(key, String(value));
    return request<TasksView>(`/api/tasks?${params}`, options);
  },
  task(taskId: string, options?: ReadRequestOptions) {
    return request<{ task: Task; history: TaskEvent[] }>(`/api/tasks/${encodeURIComponent(taskId)}`, options);
  },
  previewTaskBoard(board: TaskBoardDraft) {
    return request<{ preview: TaskBoardPreview }>('/api/task-boards/preview', { method: 'POST', readOnly: true, body: JSON.stringify({ board }) });
  },
  taskOperationReceipt(idempotencyKey: string, options?: ReadRequestOptions) {
    return request<{ receipt: { operation: string; taskId?: string; version?: number } | null }>(`/api/tasks/operations/${encodeURIComponent(idempotencyKey)}`, options);
  },
  taskBoards(options?: ReadRequestOptions) { return request<{ boards: TaskBoard[] }>('/api/task-boards', options); },
  taskWrite<T>(path: string, body: Record<string, unknown>) {
    if (!/^\/api\/(tasks(?:\/[^?#]*)?|task-series(?:\/materialize)?|task-boards)$/.test(path)) throw new Error('Ugyldig opgavehandling');
    return request<T>(path, { method: 'POST', body: JSON.stringify(body) });
  },
};
