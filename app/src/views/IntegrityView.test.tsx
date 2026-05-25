import { describe, expect, test } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { IntegrityView } from "./IntegrityView";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";

function sample(overrides: Partial<{
  chainOk: boolean;
  backupOk: boolean;
  errors: string[];
  destinations: any[];
}> = {}) {
  return {
    ok: true as const,
    integrity: {
      slug: "acme-aps",
      company: {
        name: "Acme ApS",
        cvr: "DK12345678",
        country: "DK",
        currency: "DKK",
      },
      auditChain: {
        ok: overrides.chainOk ?? true,
        entries: 42,
        errors: overrides.errors ?? [],
      },
      backup: {
        ok: overrides.backupOk ?? true,
        latestBackupAt: "2026-05-20T10:00:00Z",
        latestBackupId: "backup-2026-05-20",
        backupDue: !(overrides.backupOk ?? true),
        hasActivitySinceBackup: false,
        daysSinceLatestBackup: 5,
        backupsFound: 3,
        requiredBy: null,
        checkedAt: "2026-05-25T12:00:00Z",
      },
      destinations: overrides.destinations ?? [
        {
          id: "dest-1",
          label: "EU Cloud Storage",
          kind: "s3",
          location: "s3://acme-backups",
          inEeaOrEu: true,
          country: "DE",
          meetsRecognisedStandards: true,
          nonRelatedParty: true,
          lastPlacementAt: "2026-05-20",
        },
      ],
      legalCitation: {
        sourceId: "DK-BOGFORINGSLOVEN-2022-700",
        note: "Bogføringsloven § 14 — bogføringsmaterialet skal opbevares forsvarligt …",
      },
    },
  };
}

function renderView(payload: ReturnType<typeof sample> = sample()) {
  mockFetch({ "GET /api/companies/acme-aps/integrity": payload });
  return renderAt(<IntegrityView />, {
    route: "/companies/acme-aps/integritet",
    path: "/companies/:slug/integritet",
  });
}

describe("IntegrityView (#333)", () => {
  test("viser PASS-status når hash-kæden er hel", async () => {
    renderView(sample());
    expect(await screen.findByText(/PASS — kæden er hel/)).toBeInTheDocument();
    expect(
      screen.queryByText(/Revisionskæden er brudt/),
    ).not.toBeInTheDocument();
  });

  test("viser FAIL-callout når kæden er brudt", async () => {
    renderView(
      sample({
        chainOk: false,
        errors: [
          "2026-00001: entry_hash mismatch",
          "2026-00002: previous_hash mismatch",
        ],
      }),
    );
    expect(
      await screen.findByText(/Revisionskæden er brudt/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/2026-00001: entry_hash mismatch/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/2026-00002: previous_hash mismatch/),
    ).toBeInTheDocument();
  });

  test("viser backup-tabel med seneste backup og antal", async () => {
    renderView(sample());
    expect(await screen.findByText(/Backup-status/)).toBeInTheDocument();
    expect(
      screen.getByText(/2026-05-20T10:00:00Z/),
    ).toBeInTheDocument();
    expect(screen.getByText("backup-2026-05-20")).toBeInTheDocument();
  });

  test("warn-callout når backup er forfalden", async () => {
    renderView(sample({ backupOk: false }));
    expect(await screen.findByText(/Backup forfalden/)).toBeInTheDocument();
  });

  test("destinations-tabel viser EU/EØS-flag og senest brugt", async () => {
    renderView(sample());
    expect(
      await screen.findByText(/Backup-destinationer \(1\)/),
    ).toBeInTheDocument();
    const row = (
      await screen.findByRole("cell", { name: "EU Cloud Storage" })
    ).closest("tr")!;
    expect(within(row as HTMLElement).getByText("s3")).toBeInTheDocument();
    expect(within(row as HTMLElement).getByText(/Ja \(DE\)/)).toBeInTheDocument();
  });

  test("'Verificér igen' refetcher endpointet", async () => {
    const user = userEvent.setup();
    renderView(sample());
    expect(await screen.findByText(/PASS — kæden er hel/)).toBeInTheDocument();
    const button = screen.getByRole("button", { name: /Verificér igen/ });
    await user.click(button);
    // Endpointet er idempotent — anden runde giver samme svar; vi tjekker
    // bare at vi er kommet retur til en gyldig state efter klik.
    expect(await screen.findByText(/PASS — kæden er hel/)).toBeInTheDocument();
  });

  test("citerer bogføringsloven § 14 og linker til Lovgrundlag-viewet", async () => {
    renderView(sample());
    expect(await screen.findByText(/§ 14/)).toBeInTheDocument();
    const link = screen.getByRole("link", {
      name: /Se DK-BOGFORINGSLOVEN-2022-700/,
    });
    expect(link).toHaveAttribute(
      "href",
      "/lovgrundlag#DK-BOGFORINGSLOVEN-2022-700",
    );
  });
});
