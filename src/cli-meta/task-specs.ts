import type { CommandSpec } from "./_shared";
const commonNotes = ["Explicit --workspace or RENTEMESTER_WORKSPACE; no --company or implicit company selection. State is in the workspace task database, never a company ledger.", "With a service token, the verified principal and live company access authorize the operation. --actor is separate audit attribution; without a token this is a trusted local filesystem operation."];
const mutationNotes = ["--input is a JSON object using the same business fields as the MCP operation, without workspace, actor or confirm. Every mutation requires idempotencyKey; reuse only for identical retries.", "Existing task edits require taskId and expectedVersion; stale versions fail closed. All reads return {ok:true,errors:[],...serviceFields}; business rejection is exit 1 and malformed input is exit 2."];
const specs: Array<[string, string, string[], string[]?]> = [
  ["tasks list", "List tasks, permitted companies, boards, series and runtime.", ["--input", "--companies", "--status", "--type", "--search", "--from", "--to", "--include-archived", "--show-done", "--undated", "--unassigned"]],
  ["tasks get", "Read one task and its current version.", ["--input", "--task-id"]],
  ["tasks history", "Read preserved task history and evidence.", ["--input", "--task-id"]],
  ["tasks create", "Create explicitly scoped task work.", ["--input"], ["Input: {title,scope:{kind:'company',companySlug}|{kind:'workspace',companySlugs:[]},idempotencyKey,...optionalTaskFields}. Only title and scope are product fields required for ordinary creation."]],
  ["tasks update", "Update editable task fields atomically.", ["--input"], ["Input: {taskId,patch:{title?,description?,nextAction?,scope?,assignee?,waitingOn?,workDate?,deadline?,period?,references?,evidenceRequired?,relevance?,verificationRequired?},expectedVersion,idempotencyKey}."]],
  ["tasks move", "Move task to a common status and explicit column.", ["--input"], ["Input: {taskId,status:'open'|'in_progress'|'waiting'|'done',columnId?,workspaceColumnId?,expectedVersion,idempotencyKey}. Ambiguous column selection is rejected; moving to done retains completion/evidence gates."]],
  ["tasks complete", "Record documented completion or a visibly distinct outcome.", ["--input"], ["Input: {taskId,outcome:'completed'|'not_relevant'|'cancelled'|'exception',note,references?,expectedVersion,idempotencyKey}. This never performs payment, filing, posting or approval."]],
  ["tasks reopen", "Reopen task with preserved completion history.", ["--input"], ["Input: {taskId,reason,expectedVersion,idempotencyKey}."]],
  ["tasks sync", "Synchronize authoritative source problems idempotently.", ["--input"], ["Input: {companySlugs?,asOfDate?,idempotencyKey}. New synchronization attempts use a new key; retries of an uncertain attempt reuse its exact key/payload."]],
  ["tasks reminder", "Activate, adjust or disable an in-product reminder.", ["--input"], ["Input: {taskId,reminder:{reminderId,recipientId,enabled,kind:'deadline'|'follow_up',daysBefore?,followUpDate?,frequency:'once'|'daily',timeZone},expectedVersion,idempotencyKey}. No email or agent work is scheduled."]],
  ["tasks notifications", "Read current recipient's access-filtered reminders.", []],
  ["tasks runtime", "Read actual reminder runner state.", []],
  ["tasks run", "Run one deterministic reminder tick.", ["--input"], ["Input: {asOfDate?,idempotencyKey}; runs once and returns, without starting a persistent process."]],
  ["task-series list", "Read permitted recurring task series.", ["--input", "--companies", "--include-archived"]],
  ["task-series save", "Create or change a future recurring task series.", ["--input"], ["Input: {series:{seriesId,title,scope,template,cadence:'month'|'quarter'|'year'|'custom',every,anchor:'calendar'|'fiscal',startDate,endDate,fiscalYearStartMonth,workDayOffset,deadlineDayOffset,relevance:'relevant'|'unknown'|'activity',active},expectedVersion,idempotencyKey}. expectedVersion:0 creates; stopping or pausing preserves outstanding occurrences."]],
  ["task-series project", "Read annual-wheel expectations without creating cards.", ["--input", "--companies", "--from", "--to", "--include-archived"], ["Input: {from,to,companySlugs?,includeArchived?}. Dates use YYYY-MM-DD."]],
  ["task-series materialize", "Create due routine occurrences without duplicates.", ["--input"], ["Input: {companySlugs?,asOfDate?,idempotencyKey}."]],
  ["task-boards list", "Read permitted boards and common status mappings.", ["--input", "--companies", "--include-archived"]],
  ["task-boards preview", "Preview column changes and affected tasks.", ["--input"], ["Input: {board:{boardId,scope,columns:[{columnId,name,status,isDefault}],relocations?,previewHash?},expectedVersion?}. Returns the bound previewHash before a meaning change or populated column deletion."]],
  ["task-boards save", "Save board columns with explicit relocation and confirmation.", ["--input", "--confirm"], ["Input: {board:{boardId,scope,columns:[{columnId,name,status,isDefault}],relocations?,previewHash?},expectedVersion,idempotencyKey}. expectedVersion:0 creates; --confirm yes is always required. Use the matching previewHash for destructive column changes."]],
];
const writes = new Set(["tasks create", "tasks update", "tasks move", "tasks complete", "tasks reopen", "tasks sync", "tasks reminder", "tasks run", "task-series save", "task-series materialize", "task-boards save"]);
export const taskSpecs: CommandSpec[] = specs.map(([key, description, flags, notes = []]) => ({
  key, description,
  usage: `${key} --workspace <dir>${flags.includes("--input") ? writes.has(key) || key === "task-boards preview" ? " --input <file.json>" : " [--input <file.json>]" : ""}${writes.has(key) ? " --actor <id>" : ""}${key === "task-boards save" ? " --confirm yes" : ""}`,
  allowedFlags: ["--workspace", ...flags],
  inputNotes: [...commonNotes, ...(writes.has(key) ? mutationNotes : []), ...notes],
}));
