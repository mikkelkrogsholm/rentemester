import { type Page } from "@playwright/test";

/** Real Rust commit, deliberately lost browser reply, then read-back and exact replay. */
export async function checkNativeLostPosting(page: Page, base: string) {
  const slug = "synthetic";
  const path = `/api/companies/${slug}/manual-journal`;
  const text = "Synthetic accepted posting with lost browser reply";
  const need = (value: unknown, message: string) => { if (!value) throw new Error(message); };
  const journal = async () => {
    const response = await page.request.get(`${base}/api/companies/${slug}/journal`);
    need(response.ok(), "Canonical journal must be readable");
    return (await response.json()).journal;
  };
  const before = await journal();
  let writes = 0;
  let original: unknown;
  let accepted: { entryId: number; entryNo: string; entryHash: string; receipt: { replayed: boolean } } | undefined;
  await page.route(`**${path}`, async (route) => {
    if (route.request().method() !== "POST") return route.continue();
    writes++;
    original = route.request().postDataJSON();
    const response = await route.fetch();
    need(response.ok(), "The deliberately lost write must actually commit");
    accepted = await response.json();
    need(accepted?.receipt?.replayed === false, "Fresh intent must create one real receipt");
    await route.abort("connectionreset");
  });
  const openForm = async () => {
    await page.getByRole("button", { name: "Ny postering", exact: true }).click();
    const form = page.getByRole("region", { name: "Ny manuel postering", exact: true });
    await form.getByLabel("Posteringstekst", { exact: true }).fill(text);
    await form.getByLabel("Debetkonto", { exact: true }).selectOption("1759");
    await form.getByLabel("Kreditkonto", { exact: true }).selectOption("8759");
    await form.getByLabel("Beløb i DKK", { exact: true }).fill("9.87");
    await form.getByRole("button", { name: "Gennemse postering", exact: true }).click();
    return page.getByRole("dialog", { name: "Bogfør postering", exact: true });
  };
  try {
    await page.goto(`${base}/companies/${slug}/posteringer`);
    let dialog = await openForm();
    await dialog.getByRole("button", { name: "Bogfør postering", exact: true }).click();
    await dialog.getByRole("button", { name: "Resultatet er afklaret", exact: true }).waitFor();
    need(await dialog.getByRole("button", { name: "Bogfør postering", exact: true }).isDisabled(), "Unknown result must block a second write");
    await dialog.getByRole("button", { name: "Annullér", exact: true }).click();
    await page.getByRole("region", { name: "Ny manuel postering", exact: true }).getByRole("button", { name: "Annullér", exact: true }).click();
    await page.goto(`${base}/companies/${slug}/posteringer/`);
    dialog = await openForm();
    need(await dialog.getByRole("button", { name: "Bogfør postering", exact: true }).isDisabled(), "Full reload and equivalent route must retain the block");
    await dialog.getByRole("button", { name: "Kontrollér status", exact: true }).click();
    need(await dialog.getByRole("button", { name: "Bogfør postering", exact: true }).isDisabled(), "Read-back alone must not silently permit another write");
    const once = await journal();
    need(once.entries.length === before.entries.length + 1 && once.entries.filter((entry: { text: string }) => entry.text === text).length === 1, "Lost reply must leave exactly one canonical posting");
    need(writes === 1 && accepted && original, "Browser must have sent only the original intent");
    const replay = await page.request.post(`${base}${path}`, { headers: { origin: base }, data: original });
    need(replay.ok(), "Original envelope must reconcile through authoritative replay");
    const receipt = await replay.json();
    need(receipt.receipt?.replayed === true && receipt.entryId === accepted!.entryId && receipt.entryNo === accepted!.entryNo && receipt.entryHash === accepted!.entryHash, "Replay must return the original immutable result");
    need(JSON.stringify(await journal()) === JSON.stringify(once), "Reconciliation must not change the ledger");
    await dialog.getByRole("button", { name: "Resultatet er afklaret", exact: true }).click();
    await page.getByRole("button", { name: "Jeg har kontrolleret resultatet", exact: true }).click();
    need(await dialog.getByRole("button", { name: "Bogfør postering", exact: true }).isEnabled(), "Explicit reconciliation must release the form");
    need(writes === 1 && JSON.stringify(await journal()) === JSON.stringify(once), "Releasing the form must not repeat the posting");
    await dialog.getByRole("button", { name: "Annullér", exact: true }).click();
    await page.getByRole("region", { name: "Ny manuel postering", exact: true }).getByRole("button", { name: "Annullér", exact: true }).click();
    await page.getByText(text, { exact: true }).waitFor();
    return { acceptedReplyDiscarded: true, browserWrites: writes, blockSurvivesReloadAndRouteVariant: true, exactReceiptReplay: true, explicitReleaseWithoutWrite: true, entryId: receipt.entryId, entryHash: receipt.entryHash };
  } finally {
    await page.unroute(`**${path}`);
  }
}
