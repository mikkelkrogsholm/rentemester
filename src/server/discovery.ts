// Legacy workspace company lookup. Workspace reads never adopt directories.
//
// A workspace is a directory holding one company subdirectory per `slug`
// (`<workspace>/<slug>/`), indexed by a `workspace.json` manifest. The manifest
// is the cockpit's source of truth for "which companies exist".
//
// Arbitrary sibling directories are deliberately ignored. Registration is an
// explicit write; discovery is a read-only validation of canonical live roots.

import { requireCanonicalLiveCompanies, type WorkspaceCompanyEntry } from "../core/workspace";

/**
 * Kept as a compatibility name for callers introduced before #597. It is a
 * manifest-only read: present-but-unregistered directories remain invisible.
 */
export function discoverWorkspaceCompanies(
  workspaceRoot: string,
): WorkspaceCompanyEntry[] {
  return requireCanonicalLiveCompanies(workspaceRoot).map((company) => company.entry);
}
