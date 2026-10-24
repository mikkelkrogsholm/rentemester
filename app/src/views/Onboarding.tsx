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
          importere bankudtog (CSV) og indlæse bilag. I Rust-appen finder du
          start-saldi ved migrering i panelet Åbningsbalance under virksomhedens
          Administrér-side. Under Integritet finder du Hent workspace-backup og
          Vis backupdestinationer, hvor du kan placere og kontrollere backup på
          destinationer konfigureret af serverens administrator. Disse opgaver
          kræver ikke kommandolinjen. Lønhåndtering håndteres stadig via
          kommandolinjen.
        </p>
        <p className="muted">
          Efter eksport kontrollerer du filen i desktopappens menu Backup →
          Kontrollér eksporteret backup… med en separat betroet offentlig nøgle.
          Vejledningen under Hent workspace-backup forklarer lokal nøglelagring
          og nøgle fra hosted-operatøren. En fuld kontrol erstatter ikke en
          gendannelsesprøve i en ny mappe eller genoprettelse af login.
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
