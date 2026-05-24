// Hjælp og support — cockpittets ene synlige exit-vej for en bruger der står
// fast (#421). Top-baren peger hertil; siden samler links til www-sitets
// dokumentation, kontaktformular, GitHub-issues og en kort kom-i-gang-tjekliste.
//
// Vi holder siden bevidst statisk og uden API-kald: hjælp skal kunne nås selv
// hvis backend-API'et er nede eller bruger lige har installeret cockpittet og
// ikke kan tolke fejlmeddelelser. Indhold er på dansk og uden CLI-jargon
// (acceptkriterium fra #421).

const DOCS_BASE = "https://rentemester.dk";
const REPO_ISSUES = "https://github.com/mikkelkrogsholm/rentemester/issues";

type ExternalLinkProps = {
  href: string;
  children: React.ReactNode;
};

function ExternalLink({ href, children }: ExternalLinkProps) {
  return (
    <a href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  );
}

export function HelpView() {
  return (
    <section className="help-view">
      <header className="page-head">
        <h2>Hjælp og support</h2>
      </header>

      <p className="muted">
        Står du fast? Her er en samlet vej til dokumentation, kontakt og
        fejlrapportering. Linkene åbner i ny fane.
      </p>

      <div className="help-grid">
        <article className="card">
          <h3>Dokumentation</h3>
          <ul>
            <li>
              <ExternalLink href={`${DOCS_BASE}/saadan-virker-det`}>
                Sådan virker det
              </ExternalLink>{" "}
              — kort intro til hvordan Rentemester bogfører for dig.
            </li>
            <li>
              <ExternalLink href={`${DOCS_BASE}/funktioner`}>
                Funktioner
              </ExternalLink>{" "}
              — overblik over hvad cockpittet kan.
            </li>
            <li>
              <ExternalLink href={`${DOCS_BASE}/docs/`}>
                Brugerguide
              </ExternalLink>{" "}
              — uddybende vejledninger og FAQ.
            </li>
          </ul>
        </article>

        <article className="card">
          <h3>Kom i gang</h3>
          <p className="muted">Fire trin fra installeret cockpit til moms:</p>
          <ol>
            <li>Opret virksomheden under “Tilføj virksomhed”.</li>
            <li>Importér kontoudtog under “Bank”.</li>
            <li>Bogfør bilag og bank-bevægelser under “Posteringer”.</li>
            <li>Generér momsangivelse under “Moms” når perioden er slut.</li>
          </ol>
        </article>

        <article className="card">
          <h3>Kontakt og support</h3>
          <ul>
            <li>
              <ExternalLink href={`${DOCS_BASE}/kontakt`}>
                Kontakt
              </ExternalLink>{" "}
              — skriv direkte til teamet bag Rentemester.
            </li>
            <li>
              <ExternalLink href={REPO_ISSUES}>
                Rapportér en fejl på GitHub
              </ExternalLink>{" "}
              — åbn et issue hvis cockpittet driller.
            </li>
          </ul>
        </article>
      </div>
    </section>
  );
}
