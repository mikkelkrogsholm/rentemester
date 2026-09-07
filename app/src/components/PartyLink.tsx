import { Link } from "react-router-dom";

/** A canonical-party deep link.  It intentionally renders nothing without an
 * ID: display text is never evidence for resolving a party. */
export function PartyLink({ slug, partyId, children }: { slug: string; partyId: string | null | undefined; children: React.ReactNode }) {
  if (!partyId) return <>{children}</>;
  return <Link to={`/companies/${encodeURIComponent(slug)}/parter/${encodeURIComponent(partyId)}${location.search}`}>{children}</Link>;
}

export function PartySummary({ name, roles = [] }: { name: string; roles?: string[] }) {
  return <span>{name}{roles.length ? <span className="muted"> · {roles.join(", ")}</span> : null}</span>;
}
