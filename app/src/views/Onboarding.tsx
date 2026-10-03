import * as stylex from "@stylexjs/stylex";
import { PageHeader } from "../components/ui";
// First-run onboarding. Shown when the workspace has no companies yet — it
// explains the cockpit's scope (overview + write actions: invoicing, bank CSV
// import, document intake) and presents the create-company form.

import { CompanyForm } from "../components/CompanyForm";

export function Onboarding({
  onCreated,
}: {
  onCreated: (slug: string) => void;
}) {
  return (
    <section>
      <PageHeader title="Velkommen til Rentemester">
        <div>

          <p className="muted">
            Du har endnu ikke oprettet nogen virksomheder. Opret din første for
            at komme i gang.
          </p>
        </div>
      </PageHeader>

      <div className={["card", stylex.props(viewStyles.site0).className].filter(Boolean).join(" ")} >
        <p className={["muted", stylex.props(viewStyles.site1).className].filter(Boolean).join(" ")} >
          Cockpittet er dit kontrolpanel: du opretter og overvåger virksomheder
          her. Du kan også bogføre direkte i cockpittet — udstede fakturaer,
          importere bankudtog (CSV) og indlæse bilag. Et par mere tekniske
          opgaver — fx lønhåndtering, opsætning af backup-destinationer og
          start-saldi ved migrering — håndteres stadig via kommandolinjen
          indtil videre.
        </p>
      </div>

      <CompanyForm onCreated={onCreated} submitLabel="Opret første virksomhed" />
    </section>
  );
}

const viewStyles = stylex.create({
site0: { marginBottom: 20 },
site1: { marginTop: 0 }
});
