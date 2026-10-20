import { type Page } from "@playwright/test";

/** Hold a real, already-authorized old-company reply across client-side context changes. */
export async function checkNativeContextSwitch(page: Page, base: string, loginAsMember: () => Promise<void>) {
  const primary = "synthetic";
  const second = "synthetic-second-ledger";
  const oldMarker = `Synthetic isolated company ${primary}`;
  const newMarker = `Synthetic isolated company ${second}`;
  const need = (value: unknown, message: string) => { if (!value) throw new Error(message); };
  async function heldAuthorizedRead() {
    let ready!: () => void;
    let reject!: (error: unknown) => void;
    let release!: () => void;
    let finish!: () => void;
    let failure: unknown;
    let claimed = false;
    const arrived = new Promise<void>((resolve, fail) => { ready = resolve; reject = fail; });
    const gate = new Promise<void>((resolve) => { release = resolve; });
    const done = new Promise<void>((resolve) => { finish = resolve; });
    const timer = setTimeout(() => reject(new Error("No held authorized journal reply")), 45000);
    const pattern = `${base}/api/companies/${primary}/journal*`;
    await page.route(pattern, async (route) => {
      // Hold one observed owner reply. Later requests retain their own current
      // authorization, including expected403 after the account has changed.
      if (claimed) return route.continue();
      claimed = true;
      try {
        const response = await route.fetch();
        const body = await response.json();
        need(response.ok() && body.journal.entries.some((entry: { text: string }) => entry.text === oldMarker), "Held response must contain real previously authorized ledger data");
        clearTimeout(timer); ready();
        await gate;
        try { await route.fulfill({ response }); }
        catch (error) { if (!route.request().failure()) throw error; }
      } catch (error) { failure = error; reject(error); }
      finally { finish(); }
    });
    return {
      arrived,
      release: async () => {
        release(); await done;
        if (failure) throw failure;
        // Let the browser consume the reply and complete React's next paint.
        await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
      },
      close: async () => { clearTimeout(timer); release(); await page.unroute(pattern); },
    };
  }
  async function assertSecondOnly() {
    await page.getByText(newMarker, { exact: true }).waitFor();
    need(await page.getByText(oldMarker, { exact: true }).count() === 0, "Late old-company response must not enter the new view");
    need(new URL(page.url()).pathname === `/companies/${second}/posteringer`, "Late reply must not change destination");
  }
  let held = await heldAuthorizedRead();
  try {
    await page.goto(`${base}/companies/${primary}/posteringer`, { waitUntil: "domcontentloaded" });
    await held.arrived;
    await page.getByLabel("Skift virksomhed", { exact: true }).selectOption(second);
    await assertSecondOnly();
    await held.release();
    await assertSecondOnly();
  } finally { await held.close(); }

  held = await heldAuthorizedRead();
  try {
    await page.getByLabel("Skift virksomhed", { exact: true }).selectOption(primary);
    await held.arrived;
    await page.getByRole("group", { name: "Konto", exact: true }).getByRole("button", { name: "Log ud", exact: true }).click();
    await page.getByRole("heading", { name: "Log ind", exact: true }).waitFor();
    await loginAsMember();
    await page.getByLabel("Skift virksomhed", { exact: true }).selectOption(second);
    await assertSecondOnly();
    await held.release();
    await assertSecondOnly();
    const me = await (await page.request.get(`${base}/api/me`)).json();
    need(me.user.email === "reader@example.invalid" && me.companies.length === 1 && me.companies[0].slug === second, "New login must retain only its own identity and company grant");
  } finally { await held.close(); }
  return { realAuthorizedReplyHeld: true, companySwitchDiscardsOldReply: true, normalLogoutAndIndependentMfaLogin: true, accountSwitchDiscardsOldReply: true, restrictedMembershipPreserved: true };
}
