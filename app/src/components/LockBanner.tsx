// LockBanner — renders a BEK 205/2024 §4 backup-lock rejection kindly (#213).
//
// When a Cockpit write is refused with a 409 because the bookkeeping backup
// lock is engaged, the backend returns a curated Danish message. This banner
// surfaces it without alarm: the lock is a deliberate compliance control, not
// an error the operator did anything wrong to trigger.
//
// Shared on purpose — slices 2-4 (bank import, document intake, invoicing)
// reuse it for the same 409 from their own write paths.

import { Banner } from "./Feedback";

export function LockBanner({ message }: { message: string }) {
  return (
    <Banner kind="warning">
      <strong>Bogføringen er låst</strong>
      <p className="lock-message">{message}</p>
    </Banner>
  );
}
