import { describe, expect, test } from "bun:test";
import { screen } from "@testing-library/react";
import { Onboarding } from "./Onboarding";
import { renderAt } from "../test/render";

describe("Onboarding — first run", () => {
  test("welcomes the owner and shows the create-company form", () => {
    renderAt(<Onboarding onCreated={() => {}} />);
    expect(
      screen.getByText(/Velkommen til Rentemester/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("form", { name: /Opret virksomhed/i }),
    ).toBeInTheDocument();
  });

  test("the scope text reflects the Cockpit's write actions (#255)", () => {
    renderAt(<Onboarding onCreated={() => {}} />);
    // The stale claim that bookkeeping happens only via the CLI must be gone…
    expect(
      screen.queryByText(/sker fortsat via\s+agenten og kommandolinjen/i),
    ).not.toBeInTheDocument();
    // …and the text must say the Cockpit itself can book.
    const scope = screen.getByText(/bogføre direkte i cockpittet/i);
    expect(scope).toBeInTheDocument();
    expect(scope.textContent).toMatch(/fakturaer/i);
    expect(scope.textContent).toMatch(/bankudtog/i);
    expect(scope.textContent).toMatch(/bilag/i);
  });
});

test("points the new owner to existing balance and backup panels without conflating verification and restore", () => {
  renderAt(<Onboarding onCreated={() => {}} />);
  const scope = screen.getByText(/bogføre direkte i cockpittet/i);
  expect(scope).toHaveTextContent(/Åbningsbalance under virksomhedens Administrér-side/);
  expect(scope).toHaveTextContent(/Under Integritet.*Hent workspace-backup.*Vis backupdestinationer/);
  expect(scope).toHaveTextContent(/konfigureret af serverens administrator/);
  expect(scope).not.toHaveTextContent(/backup-destinationer og start-saldi.*via kommandolinjen/);
  const verification = screen.getByText(/Efter eksport kontrollerer du filen/);
  expect(verification).toHaveTextContent(/desktopappens menu Backup → Kontrollér eksporteret backup….*separat betroet offentlig nøgle/);
  expect(verification).toHaveTextContent(/erstatter ikke en gendannelsesprøve.*genoprettelse af login/);
});
