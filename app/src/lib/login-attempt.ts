import { authClient } from "./auth-client";

type Identity = { id: string; email: string; emailVerified: boolean; twoFactorEnabled?: boolean };
export type LoginAttempt = { readonly email: string; id?: string; ready: boolean };
type Status = "idle" | "active" | "cleaning" | "blocked";

/** Process-wide cookie fence: survives LoginView unmounts. Never abort a cookie mutation
 * and mistake that for settlement. Cleanup runs only after its response has settled. */
export class LoginAttempts {
  private attempt: LoginAttempt | null = null;
  private pending: Promise<unknown> | null = null;
  private cleanup: Promise<void> | null = null;
  private status: Status = "idle";
  private listeners = new Set<() => void>();
  epoch = 0;
  constructor(private readonly signOut: () => Promise<{ error?: unknown }>, private readonly read: () => Promise<{ error?: unknown; data?: { user?: unknown } | null }>) {}
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  snapshot = () => this.status;
  private change(status: Status) { this.status = status; this.epoch++; this.listeners.forEach(listener => { listener(); }); }
  start(email: string): LoginAttempt {
    if (this.status !== "idle") throw new Error("Authentication cleanup required");
    const attempt = { email: email.trim().toLowerCase(), ready: false };
    this.attempt = attempt; this.change("active"); return attempt;
  }
  current(attempt: LoginAttempt) { return this.status === "active" && this.attempt === attempt; }
  async run<T>(attempt: LoginAttempt, effect: () => Promise<T>): Promise<T> {
    if (!this.current(attempt) || this.pending) throw new Error("Authentication attempt invalidated");
    const pending = Promise.resolve().then(effect);
    this.pending = pending;
    try { return await pending; } finally { if (this.pending === pending) this.pending = null; }
  }
  remember(attempt: LoginAttempt, user: { id?: string; email?: string } | undefined) {
    if (!this.current(attempt) || !user) return;
    if (!user.id || user.email?.trim().toLowerCase() !== attempt.email || (attempt.id && attempt.id !== user.id)) throw new Error("Account mismatch");
    attempt.id = user.id;
  }
  ready(attempt: LoginAttempt) { if (!this.current(attempt)) throw new Error("Cancelled"); attempt.ready = true; }
  assertSession(user: Identity | null) {
    if (this.status === "idle") return;
    const attempt = this.attempt;
    if (this.status !== "active" || !attempt?.ready || !user || !user.id || user.email.trim().toLowerCase() !== attempt.email || (attempt.id && attempt.id !== user.id) || !user.emailVerified || !user.twoFactorEnabled) throw new Error("Selected account was not confirmed");
    attempt.id = user.id;
  }
  completeAdoption() { if (this.attempt?.ready) this.finish(this.attempt); }
  finish(attempt: LoginAttempt) { if (this.current(attempt)) { this.attempt = null; this.change("idle"); } }
  cancel(attempt?: LoginAttempt): Promise<void> {
    if (attempt && !this.current(attempt)) return this.cleanup ?? Promise.resolve();
    if (this.cleanup) return this.cleanup;
    if (this.status === "idle") return Promise.resolve();
    this.attempt = null;
    this.change("cleaning");
    if (typeof window !== "undefined") window.dispatchEvent(new Event("rentemester:auth-expired"));
    const pending = this.pending;
    this.cleanup = (async () => {
      try {
        await pending?.catch(() => undefined);
        const result = await this.signOut();
        if (result.error) throw new Error("Signout unconfirmed");
        const session = await this.read();
        if (session.error || session.data !== null) throw new Error("Cookie cleanup unconfirmed");
        this.change("idle");
      } catch { this.change("blocked"); }
      finally { this.cleanup = null; }
    })();
    return this.cleanup;
  }
}

export const loginAttempts = new LoginAttempts(
  () => authClient.signOut({ fetchOptions: { retry: 0 } }),
  () => authClient.getSession({ fetchOptions: { retry: 0, cache: "no-store" } }),
);
