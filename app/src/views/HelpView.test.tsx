// Verifies that HelpView's exit-paths actually lead somewhere real:
//
// 1) #445 — "Brugerguide" must not link to `${DOCS_BASE}/docs/`, which is a
//    404 (www/src/pages/docs/ has no index.astro, only installation.astro).
// 2) #446 — the "Kom i gang" checklist must not tell the owner to post under
//    "Posteringer" (which is read-only). Posting receipts lives under "Bilag"
//    and bank reconciliation under "Bank" — both labels come from CompanyNav.

import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import { HelpView } from "./HelpView";
import { renderAt } from "../test/render";

function renderHelp() {
  return renderAt(<HelpView />, { route: "/help", path: "/help" });
}

describe("HelpView — Brugerguide-link (#445)", () => {
  test("Brugerguide peger ikke på den blinde /docs/-forside", () => {
    renderHelp();
    const link = screen.getByRole("link", { name: /Brugerguide/ });
    const href = link.getAttribute("href") ?? "";
    // /docs/ root is a 404; only /docs/installation exists.
    expect(href).not.toMatch(/\/docs\/?$/);
  });

  test("Brugerguide lander på en eksisterende side", () => {
    renderHelp();
    const link = screen.getByRole("link", { name: /Brugerguide/ });
    const href = link.getAttribute("href") ?? "";
    // Accept either the existing installation guide, or a GitHub docs URL.
    expect(href).toMatch(/\/docs\/installation|github\.com\/.+\/(docs|tree|blob)/);
  });
});

describe("HelpView — Kom i gang routing (#446)", () => {
  test("trin 3 sender ikke ejeren til Posteringer for at bogføre", () => {
    renderHelp();
    const card = screen.getByRole("heading", { name: /Kom i gang/ }).closest(
      "article",
    )!;
    const items = within(card as HTMLElement).getAllByRole("listitem");
    // The step that mentions bookkeeping receipts must NOT point at
    // "Posteringer" — that view is read-only.
    const bogforStep = items.find((li) => /Bogf(ø|o)r.*bilag/i.test(li.textContent ?? ""));
    expect(bogforStep, "fandt ikke et trin om at bogføre bilag").toBeTruthy();
    expect(bogforStep!.textContent).not.toMatch(/Posteringer/);
  });

  test("Kom-i-gang henviser til Bilag og Bank som faktiske bogføringsfaner", () => {
    renderHelp();
    const card = screen.getByRole("heading", { name: /Kom i gang/ }).closest(
      "article",
    )!;
    const text = (card as HTMLElement).textContent ?? "";
    // Tab labels come from CompanyNav.tsx — Bilag (DocumentsView) and Bank
    // (BankView) are where posting/reconciliation actually happen.
    expect(text).toMatch(/Bilag/);
    expect(text).toMatch(/Bank/);
  });
});
