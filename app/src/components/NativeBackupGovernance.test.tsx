import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeBackupGovernance } from "./NativeBackupGovernance";
function view(hash: string, observed: boolean) {
  return { ok: true, slug: "synthetic", backup: { backupDue: false, latestBackupAt: "2026-10-21T00:00:00Z", latestBackupId: hash, checkedAt: "2026-10-21T01:00:00Z", errors: [] }, lock: { locked: false },
    governance: { format: "rentemester-native-backup-governance/1", latestBackupId: hash, latestBackupExternalCopyObserved: observed, latestBackupExternalCopyAtDeclaredDestination: false, attestationIndependentlyVerified: false, recoveryVerified: false, errors: [], destinations: [{ id: "synthetic-drive", label: "Synthetic Drive", offsiteAttested: false, placements: observed ? [{}] : [], remoteVerifications: observed ? [{ operationId: "synthetic-remote-115", archiveSha256: hash, archiveSizeBytes: 123, observedAt: "2026-10-21T00:30:00Z" }] : [] }] } };
}
test("explicit read shows exact backup evidence and newer backup loses old external-copy claim", async () => {
  let newer = false;
  const fetch = vi.fn(async (_path: string | URL, init?: RequestInit) => {
    expect(init?.method).not.toBe("POST");
    return Response.json(view((newer ? "b" : "a").repeat(64), !newer));
  }); stubGlobal("fetch", fetch); renderAt(<NativeBackupGovernance slug="synthetic" />);
  expect(fetch).toHaveBeenCalledTimes(0);
  await userEvent.click(screen.getByRole("button", { name: "Hent samlet backupstatus" }));
  await screen.findByText("Ekstern kopi observeret");
  expect(screen.getByText("a".repeat(64))).toBeInTheDocument();
  expect(screen.getByText(/ikke uafhængigt verificeret/)).toBeInTheDocument();
  newer = true;
  await userEvent.click(screen.getByRole("button", { name: "Hent samlet backupstatus" }));
  await screen.findByText("Ingen verificeret ekstern kopi til denne backup");
  expect(screen.getByText("b".repeat(64))).toBeInTheDocument();
  expect(screen.queryByText("Ekstern kopi observeret")).not.toBeInTheDocument();
  expect(fetch).toHaveBeenCalledTimes(2);
});
test("a response with a remote receipt for a different backup is refused", async () => {
  const value = view("a".repeat(64), true);
  value.governance.destinations[0]!.remoteVerifications[0]!.archiveSha256 = "b".repeat(64);
  stubGlobal("fetch", vi.fn(async () => Response.json(value))); renderAt(<NativeBackupGovernance slug="synthetic" />);
  await userEvent.click(screen.getByRole("button", { name: "Hent samlet backupstatus" }));
  await screen.findByRole("alert");
  expect(screen.queryByText("Ekstern kopi observeret")).not.toBeInTheDocument();
});
