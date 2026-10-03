import { ButtonLink, PageHeader } from "../components/ui";
// Standalone "add company" route — the same form as onboarding, reached from
// the portfolio when the workspace is already populated.

import { useNavigate } from "react-router-dom";
import { CompanyForm } from "../components/CompanyForm";

export function AddCompanyView() {
  const navigate = useNavigate();
  return (
    <section>
      <PageHeader title="Tilføj virksomhed" actions={<><ButtonLink className="btn secondary" to="/">
          Annullér
        </ButtonLink></>}>
        <div>

          <p className="muted">
            Registrerer en ny virksomhed i arbejdsområdet og opretter dens
            regnskab.
          </p>
        </div>

      </PageHeader>
      <CompanyForm onCreated={(slug) => navigate(`/companies/${slug}`)} />
    </section>
  );
}
