import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import { RulesView } from "./RulesView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

const sample = {
  ok: true as const,
  ruleBundles: [
    {
      name: "dk-bookkeeping",
      version: "dk-bookkeeping-v0.0.6",
      ruleCount: 3,
      sources: ["DK-BOGFORINGSLOVEN-2022-700"],
      vatCodes: [],
    },
    {
      name: "dk-vat",
      version: "dk-vat-v0.0.3",
      ruleCount: 2,
      sources: ["DK-MOMSLOVEN-2024-209"],
      vatCodes: ["DK_PURCHASE_25", "DK_SALE_25"],
    },
  ],
  rules: [
    {
      ruleId: "DK-BOOK-001",
      bundle: "dk-bookkeeping",
      sourceId: "DK-BOGFORINGSLOVEN-2022-700",
      name: "Append-only-bogføring",
      explanation: "Posteringer må ikke ændres efter de er godkendt.",
      severity: "blocker",
      category: "bookkeeping",
      provisions: [
        {
          ref: "§ 9, stk. 1",
          textHash: "sha256:" + "a".repeat(64),
        },
      ],
    },
    {
      ruleId: "DK-VAT-001",
      bundle: "dk-vat",
      sourceId: "DK-MOMSLOVEN-2024-209",
      name: "Almindelig dansk købsmoms 25 %",
      explanation: "Indenlandske køb løfter 25 % moms.",
      severity: "info",
      category: "vat",
      provisions: [
        {
          ref: "§ 33",
          textHash: "sha256:" + "b".repeat(64),
        },
      ],
    },
  ],
  legalSources: [
    {
      id: "DK-BOGFORINGSLOVEN-2022-700",
      title: "Lov om bogføring",
      authority: "Erhvervsministeriet",
      category: "bookkeeping",
      url: "https://www.retsinformation.dk/eli/lta/2022/700",
    },
    {
      id: "DK-MOMSLOVEN-2024-209",
      title: "Bekendtgørelse af lov om merværdiafgift (momsloven)",
      authority: "Skatteministeriet",
      category: "vat",
      url: "https://www.retsinformation.dk/eli/lta/2024/209",
    },
  ],
};

function renderView() {
  mockFetch({ "GET /api/rules": sample });
  return renderAt(<RulesView />, { route: "/lovgrundlag", path: "/lovgrundlag" });
}

describe("RulesView (#347)", () => {
  test("viser bundler med versionsstreng og antal regler", async () => {
    renderView();
    // Vent på indlæsning af tabellen — versionsstrengen er entydig så den er en
    // sikker anchor i hele DOM'en.
    expect(
      await screen.findByText("dk-bookkeeping-v0.0.6"),
    ).toBeInTheDocument();
    expect(screen.getByText("dk-vat-v0.0.3")).toBeInTheDocument();
  });

  test("hver regel viser sit ruleId, navn og forklaring", async () => {
    renderView();
    expect(await screen.findByText(/Append-only-bogføring/)).toBeInTheDocument();
    expect(
      screen.getByText(/Posteringer må ikke ændres/),
    ).toBeInTheDocument();
    expect(screen.getByText("DK-BOOK-001")).toBeInTheDocument();
  });

  test("provisions har en SHA-256-fingeraftryk pr. paragraf", async () => {
    renderView();
    // Provisions findes inde i `<details>` — åbn dem via klik for at finde
    // SHA-strengen i DOM'en.
    const detailsList = await screen.findAllByText(/Citationer/);
    for (const el of detailsList) {
      (el.closest("details") as HTMLDetailsElement).open = true;
    }
    expect(
      screen.getByText(new RegExp(`sha256:${"a".repeat(8)}`)),
    ).toBeInTheDocument();
  });

  test("regelteller respekterer bundle-filter", async () => {
    renderView();
    await screen.findByText(/Regler \(2 af 2\)/);
    // Filtrér til dk-vat — der findes kun én regel i fixture'n.
    const select = screen.getByRole("combobox");
    (select as HTMLSelectElement).value = "dk-vat";
    select.dispatchEvent(new Event("change", { bubbles: true }));
    expect(await screen.findByText(/Regler \(1 af 2\)/)).toBeInTheDocument();
  });

  test("siden er read-only — ingen oprettelses-/redigerings-knapper", async () => {
    renderView();
    await screen.findByText(/Lovgrundlag/);
    expect(
      screen.queryByRole("button", { name: /Opret|Rediger|Slet|Tilføj/ }),
    ).not.toBeInTheDocument();
  });

  test("hver bundle viser sine kilder", async () => {
    renderView();
    expect(
      await screen.findByText(/DK-BOGFORINGSLOVEN-2022-700/),
    ).toBeInTheDocument();
    expect(screen.getByText(/DK-MOMSLOVEN-2024-209/)).toBeInTheDocument();
  });

  test("provisions linker til retsinformation.dk via legal-source URL'en", async () => {
    renderView();
    const detailsList = await screen.findAllByText(/Citationer/);
    for (const el of detailsList) {
      (el.closest("details") as HTMLDetailsElement).open = true;
    }
    const link = screen.getByRole("link", { name: /Lov om bogføring/ });
    expect(link).toHaveAttribute(
      "href",
      "https://www.retsinformation.dk/eli/lta/2022/700",
    );
  });
});
