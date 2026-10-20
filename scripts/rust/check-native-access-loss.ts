import { type Page, type Route } from "@playwright/test";

/** Revocation wins over browser reads/writes held before reaching the real server. */
export async function checkNativeAccessLoss(owner: Page, member: Page, base: string) {
  const slug = "synthetic-second-ledger";
  const prefix = `${base}/api/companies/${slug}`;
  const need = (value: unknown, message: string) => { if (!value) throw new Error(message); };
  const me = await (await member.request.get(`${base}/api/me`)).json();
  need(me.user?.id, "Normally enrolled member identity required");
  async function membership(action: "grant" | "revoke", role = "bookkeeper") {
    const response = await owner.request.post(`${base}/api/workspace/members/company`, {
      headers: { origin: base },
      data: { userId: me.user.id, action, companySlug: slug, ...(action === "grant" ? { role } : {}) },
    });
    need(response.ok(), `Owner ${action} must succeed`);
  }
  async function journal() {
    const response = await owner.request.get(`${prefix}/journal`);
    need(response.ok(), "Owner must retain canonical read access");
    return response.json();
  }
  async function hold(path: string, method: string) {
    let arrive!: () => void;
    let release!: () => void;
    let timer: ReturnType<typeof setTimeout>;
    const arrived = new Promise<void>((resolve, reject) => {
      timer = setTimeout(() => reject(new Error(`No pending ${method} ${path} request`)), 45000);
      arrive = () => { clearTimeout(timer); resolve(); };
    });
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const pattern = `${prefix}/${path}*`;
    const handler = async (route: Route) => {
      if (route.request().method() !== method) return route.continue();
      arrive();
      await gate;
      const response = await route.fetch();
      await route.fulfill({ response });
    };
    await member.route(pattern, handler);
    return { arrived, release, close: () => { clearTimeout(timer); return member.unroute(pattern, handler); } };
  }
  await membership("grant");
  await member.goto(`${base}/companies/${slug}/posteringer`);
  await member.getByRole("button", { name: "Ny postering", exact: true }).waitFor();
  const before = await journal();
  const read = await hold("journal", "GET");
  try {
    const response = member.waitForResponse((r) => new URL(r.url()).pathname === `/api/companies/${slug}/journal`);
    const navigation = member.reload({ waitUntil: "domcontentloaded" });
    await read.arrived;
    await membership("revoke");
    read.release();
    await navigation;
    need((await response).status() === 403, "Pending read must recheck revoked company membership");
    await member.getByText("Henter posteringer…", { exact: true }).waitFor({ state: "hidden" });
    need(!(await member.locator("body").innerText()).includes(`Synthetic isolated company ${slug}`), "Refused read must not expose the old ledger");
  } finally { read.release(); await read.close(); }

  await membership("grant");
  await member.reload();
  await member.getByRole("button", { name: "Ny postering", exact: true }).click();
  const form = member.getByRole("region", { name: "Ny manuel postering", exact: true });
  await form.getByLabel("Posteringstekst", { exact: true }).fill("Synthetic revoked pending write must not post");
  await form.getByLabel("Debetkonto", { exact: true }).selectOption("1759");
  await form.getByLabel("Kreditkonto", { exact: true }).selectOption("8759");
  await form.getByLabel("Beløb i DKK", { exact: true }).fill("7.65");
  await form.getByRole("button", { name: "Gennemse postering", exact: true }).click();
  const write = await hold("manual-journal", "POST");
  try {
    const response = member.waitForResponse((r) => new URL(r.url()).pathname === `/api/companies/${slug}/manual-journal` && r.request().method() === "POST");
    await member.getByRole("dialog", { name: "Bogfør postering", exact: true }).getByRole("button", { name: "Bogfør postering", exact: true }).click();
    await write.arrived;
    await membership("revoke");
    write.release();
    need((await response).status() === 403, "Pending write must recheck revoked company membership");
    need(JSON.stringify(await journal()) === JSON.stringify(before), "Revoked pending write must have no ledger effect");
  } finally { write.release(); await write.close(); }
  await membership("grant", "reader");
  return { normalMemberPromotedToBookkeeper: true, pendingReadRefusedAfterRevocation: true, pendingWriteRefusedAfterRevocation: true, canonicalLedgerUnchanged: true };
}
