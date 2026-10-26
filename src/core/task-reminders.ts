import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { canonicalJson } from "./canonical-json";
import { addDays, isValidIsoDate } from "./dates";
import { getWorkspaceUserAccess } from "./workspace-access";
import { executeTaskMutation, getTask, listTasks, TaskError, updateTask } from "./tasks";
import type { Task, TaskMutationContext, TaskNotification, TaskReminder } from "./tasks-types";

function knownRecipient(db: Database, recipientId: string): boolean {
  // The application layer allows this identity only in a trusted local deployment.
  if (recipientId === "local") return true;
  return Boolean(db.query('SELECT id FROM "user" WHERE id=?').get(recipientId)) && getWorkspaceUserAccess(db, recipientId).active;
}

function validateReminder(reminder: TaskReminder): TaskReminder {
  if (!reminder || !reminder.reminderId?.trim() || reminder.reminderId.length > 160
    || !reminder.recipientId?.trim() || reminder.recipientId.length > 160 || typeof reminder.enabled !== "boolean"
    || !["deadline", "follow_up"].includes(reminder.kind) || !["once", "daily"].includes(reminder.frequency)
    || (reminder.kind === "deadline" && (!Number.isInteger(reminder.daysBefore ?? 0) || (reminder.daysBefore ?? 0) < 0 || (reminder.daysBefore ?? 0) > 365))
    || (reminder.kind === "follow_up" && !isValidIsoDate(reminder.followUpDate ?? ""))) throw new TaskError("invalid_input", "Påmindelsen kræver gyldig modtager, dato og frekvens.");
  const timeZone = reminder.timeZone?.trim() || "Europe/Copenhagen";
  try { new Intl.DateTimeFormat("en-CA", { timeZone }).format(new Date()); } catch { throw new TaskError("invalid_input", "Påmindelsens tidszone er ugyldig."); }
  return { ...reminder, timeZone, ...(reminder.kind === "deadline" ? { daysBefore: reminder.daysBefore ?? 0 } : {}) };
}

export function configureTaskReminder(db: Database, input: { taskId: string; reminder: TaskReminder }, ctx: TaskMutationContext): Task {
  return executeTaskMutation(db, "reminder-set", input, ctx, now => {
    const reminder = validateReminder(input.reminder);
    const task = getTask(db, input.taskId);
    if (!task) throw new TaskError("not_found", "Opgaven kan ikke genfindes.");
    if (reminder.enabled && !knownRecipient(db, reminder.recipientId)) throw new TaskError("invalid_input", "Påmindelsens modtager skal være en aktiv workspacebruger.");
    if (reminder.enabled && reminder.kind === "deadline" && !task.deadline) throw new TaskError("invalid_input", "Opgaven skal have en frist før en fristpåmindelse kan aktiveres.");
    return updateTask(db, task.taskId, { reminders: [...task.reminders.filter(item => item.reminderId !== reminder.reminderId), reminder] },
      { ...ctx, now, idempotencyKey: `${ctx.idempotencyKey}:task` });
  });
}

function localDate(now: Date, timeZone: string): string {
  const parts = new Intl.DateTimeFormat("en-CA", { timeZone, year: "numeric", month: "2-digit", day: "2-digit" }).formatToParts(now);
  const value = (type: string) => parts.find(part => part.type === type)?.value ?? "";
  return `${value("year")}-${value("month")}-${value("day")}`;
}

function schedule(task: Task, reminder: TaskReminder): { due: string; signature: string } | null {
  if (!reminder.enabled || task.status === "done" || task.relevance === "not_relevant") return null;
  const due = reminder.kind === "deadline"
    ? task.deadline ? addDays(task.deadline.date, -(reminder.daysBefore ?? 0)) : null
    : reminder.followUpDate ?? null;
  if (!due || !isValidIsoDate(due)) return null;
  const signature = createHash("sha256").update(canonicalJson({ reminder, due,
    deadline: reminder.kind === "deadline" ? task.deadline : null })).digest("hex").slice(0, 24);
  return { due, signature };
}

/** Only current, explicitly enabled notifications are visible. Access is filtered by the service. */
export function listTaskNotifications(db: Database, recipientId: string): TaskNotification[] {
  if (!knownRecipient(db, recipientId)) return [];
  const rows = db.query("SELECT payload_json FROM rm_task_notifications WHERE recipient_id=? ORDER BY created_at DESC,notification_id")
    .all(recipientId) as Array<{ payload_json: string }>;
  return rows.flatMap(row => {
    const notification = JSON.parse(row.payload_json) as TaskNotification;
    const task = getTask(db, notification.taskId);
    if (!task) return [];
    const reminder = task.reminders.find(item => item.reminderId === notification.reminderId && item.recipientId === recipientId);
    const current = reminder ? schedule(task, reminder) : null;
    if (!current || !notification.slotKey.startsWith(`${current.signature}:`)) return [];
    return [{ ...notification, title: task.title }];
  });
}

/** Durable slots make retries and restarts quiet. Catch-up emits one current slot, never a backlog burst. */
export function runTaskReminders(db: Database, input: { now: string | Date; canRead: (recipientId: string, task: Task) => boolean }): { delivered: number } {
  const now = new Date(input.now);
  if (!Number.isFinite(now.getTime())) throw new TaskError("invalid_input", "Påmindelseskørslen kræver et gyldigt tidspunkt.");
  return db.transaction(() => {
    let delivered = 0;
    for (const task of listTasks(db, { showDone: false })) {
      for (const rawReminder of task.reminders) {
        const reminder = validateReminder(rawReminder);
        const current = schedule(task, reminder);
        if (!current || !knownRecipient(db, reminder.recipientId) || !input.canRead(reminder.recipientId, task)) continue;
        const today = localDate(now, reminder.timeZone);
        if (today < current.due) continue;
        const slotKey = `${current.signature}:${reminder.frequency === "once" ? "once" : today}`;
        const notificationId = `notification-${createHash("sha256").update(canonicalJson([reminder.recipientId, task.taskId, reminder.reminderId, slotKey])).digest("hex").slice(0, 32)}`;
        const notification: TaskNotification = { notificationId, recipientId: reminder.recipientId, taskId: task.taskId,
          reminderId: reminder.reminderId, slotKey, createdAt: now.toISOString(), title: task.title };
        const result = db.query("INSERT OR IGNORE INTO rm_task_notifications(notification_id,recipient_id,task_id,reminder_id,slot_key,payload_json,created_at) VALUES(?,?,?,?,?,?,?)")
          .run(notificationId, reminder.recipientId, task.taskId, reminder.reminderId, slotKey, canonicalJson(notification), notification.createdAt);
        delivered += result.changes;
      }
    }
    return { delivered };
  }).immediate();
}
