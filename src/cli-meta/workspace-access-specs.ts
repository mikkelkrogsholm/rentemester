import type { CommandSpec } from "./_shared";

export const workspaceAccessSpecs: CommandSpec[] = [{
  key: "workspace-access bootstrap-first",
  usage: "workspace-access bootstrap-first --workspace <dir> --company <slug> --name <text> --email <mail> --password-file <path> --confirm yes --actor <id>",
  description: "Opretter den første private hosted-bruger for et workspace. Offentlig signup findes ikke.",
  allowedFlags: ["--workspace", "--company", "--name", "--email", "--password-file", "--confirm"],
  inputNotes: [
    "Kræver hosted Better Auth- og http-json-v1-konfiguration samt en actor godkendt i den valgte virksomheds policy.",
    "Password læses kun fra en almindelig fil med præcis 0600-rettigheder; filsti og indhold vises aldrig i output.",
    "--confirm yes kræves før password-filen eller databasen læses.",
  ],
}, {
  key: "workspace-access bootstrap-local-service",
  usage: "workspace-access bootstrap-local-service --workspace <dir> --company <slug> --display-name <text> --company-role <owner|bookkeeper|reviewer|reader> --auth-secret-file <path> --confirm yes --actor <id>",
  description: "Opretter den første lokale, autentificerede servicekonto med eksplicit mindste virksomhedsrolle. Credential vises kun én gang.",
  allowedFlags: ["--workspace", "--company", "--display-name", "--company-role", "--auth-secret-file", "--confirm"],
  inputNotes: ["Auth-secret læses kun fra en almindelig fil med præcis 0600-rettigheder og vises aldrig igen.", "Actor er audit-identitet og giver ikke i sig selv adgang; adgang kommer fra den oprettede servicekontos live membership.", "Brug local-service-rotate eller local-service-revoke med credential-id ved rotation eller tilbagekaldelse."],
}, {
  key: "workspace-access local-service-rotate",
  usage: "workspace-access local-service-rotate --workspace <dir> --company <slug> --service-account-id <id> --credential-id <id> --auth-secret-file <path> --confirm yes --actor <id>",
  description: "Roterer ét lokalt servicecredential; den nye nøgle vises kun én gang.",
  allowedFlags: ["--workspace", "--company", "--service-account-id", "--credential-id", "--auth-secret-file", "--confirm"],
}, {
  key: "workspace-access local-service-revoke",
  usage: "workspace-access local-service-revoke --workspace <dir> --company <slug> --service-account-id <id> --credential-id <id> --auth-secret-file <path> --confirm yes --actor <id>",
  description: "Tilbagekalder ét lokalt servicecredential uden at ændre dets historiske auditspor.",
  allowedFlags: ["--workspace", "--company", "--service-account-id", "--credential-id", "--auth-secret-file", "--confirm"],
}];
