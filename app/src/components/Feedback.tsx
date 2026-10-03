import { Button } from "./ui";
// Small shared presentational components for loading / error / empty states
// and inline banners. Kept tiny and prop-driven so views stay declarative.

export function Loading({ label = "Indlæser…" }: { label?: string }) {
  return <div className="state-msg" role="status">{label}</div>;
}

export function ErrorState({
  message,
  onRetry,
}: {
  message: string;
  onRetry?: () => void;
}) {
  return (
    <div className="state-msg">
      <p className="banner error" role="alert">
        {message}
      </p>
      {onRetry && (
        <Button className="btn secondary" type="button" onClick={onRetry}>
          Prøv igen
        </Button>
      )}
    </div>
  );
}

export function Banner({
  kind,
  children,
}: {
  kind: "error" | "success" | "warning";
  children: React.ReactNode;
}) {
  return (
    <div className={`banner ${kind}`} role={kind === "error" ? "alert" : "status"}>
      {children}
    </div>
  );
}
