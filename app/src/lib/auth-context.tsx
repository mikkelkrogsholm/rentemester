import { loginAttempts } from "./login-attempt";
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { authClient } from "./auth-client";
import { ApiError, request } from "./api/_shared";
import type { CompanyRole, WorkspaceRole } from "../../../src/core/access-permissions";

export type SessionUser = {
  id: string;
  email: string;
  emailVerified: boolean;
  twoFactorEnabled?: boolean;
};

export type SessionContext = {
  user: SessionUser;
  workspaceRole: WorkspaceRole;
  companies: Array<{ slug: string; name: string; role: CompanyRole; archived: boolean }>;
};

export type AuthState = {
  backend?: "rust";
  loading: boolean;
  hosted: boolean;
  session: SessionUser | null;
  currentSessionId: string | null;
  context: SessionContext | null;
  bootstrapError?: string | null;
  refresh: () => Promise<void>;
  clear: () => void;
};

const AuthContext = createContext<AuthState | null>(null);

async function currentSession(): Promise<{ user: SessionUser; sessionId: string | null } | null> {
  const result = await authClient.getSession();
  if (result.error) {
    if (result.error.status === 401) return null;
    throw new ApiError("auth_unavailable", "Din session kunne ikke kontrolleres. Prøv igen.", result.error.status ?? 0);
  }
  if (!result.data?.user) return null;
  return {
    user: result.data.user as SessionUser,
    sessionId: typeof result.data.session?.id === "string" ? result.data.session.id : null,
  };
}

export function AuthProvider({ hosted, backend, children }: { hosted: boolean; backend?: "rust"; children: ReactNode }) {
  const [loading, setLoading] = useState(hosted);
  const [session, setSession] = useState<SessionUser | null>(null);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);
  const [context, setContext] = useState<SessionContext | null>(null);
  const [bootstrapError, setBootstrapError] = useState<string | null>(null);
  const generation = useRef(0);
  const contextRead = useRef<AbortController | null>(null);

  const clear = useCallback(() => {
    generation.current++;
    contextRead.current?.abort();
    contextRead.current = null;
    setSession(null);
    setCurrentSessionId(null);
    setContext(null);
    setBootstrapError(null);
    setLoading(false);
  }, []);

  const refresh = useCallback(async () => {
    const currentGeneration = ++generation.current;
    contextRead.current?.abort();
    const controller = new AbortController();
    contextRead.current = controller;
    if (!hosted) {
      setLoading(false);
      return;
    }
    if (loginAttempts.snapshot() !== "active") setLoading(true);
    setBootstrapError(null);
    if (["cleaning", "blocked"].includes(loginAttempts.snapshot())) { clear(); return; }
    try {
      const attemptEpoch = loginAttempts.epoch;
      const current = await currentSession();
      if (attemptEpoch !== loginAttempts.epoch) return;
      try { loginAttempts.assertSession(current?.user ?? null); }
      catch { await loginAttempts.cancel(); throw new Error("Account not confirmed"); }
      if (generation.current !== currentGeneration) return;
      const user = current?.user ?? null;
      if (loginAttempts.snapshot() !== "active") setSession(user);
      setCurrentSessionId(current?.sessionId ?? null);
      if (!user || !user.emailVerified || !user.twoFactorEnabled) {
        setContext(null);
        return;
      }
      const value = await request<SessionContext & { ok: true }>("/api/me", { signal: controller.signal });
      if (generation.current !== currentGeneration) return;
      loginAttempts.assertSession(value.user);
      if (value.user.id !== user.id) throw new ApiError("auth_unavailable", "Din session og adgang stemmer ikke overens. Log ind igen.", 0);
      loginAttempts.completeAdoption();
      setSession(user);
      setContext({ user: value.user, workspaceRole: value.workspaceRole, companies: value.companies });
    } catch (error) {
      if (generation.current !== currentGeneration) return;
      if (loginAttempts.snapshot() === "active") await loginAttempts.cancel();
      if (loginAttempts.snapshot() === "cleaning" || loginAttempts.snapshot() === "blocked") { clear(); }
      else if (error instanceof ApiError && (error.status === 401 || error.code === "unauthorized")) clear();
      else {
        // Unknown authentication is not a logout or permission to show stale books.
        setContext(null);
        setBootstrapError("Din session og adgang kunne ikke kontrolleres. Prøv igen.");
      }
    } finally {
      if (generation.current === currentGeneration) {
        contextRead.current = null;
        setLoading(false);
      }
    }
  }, [clear, hosted]);

  useEffect(() => {
    void refresh();
    return () => { generation.current++; contextRead.current?.abort(); contextRead.current = null; };
  }, [refresh]);
  useEffect(() => {
    const onExpired = () => clear();
    window.addEventListener("rentemester:auth-expired", onExpired);
    return () => window.removeEventListener("rentemester:auth-expired", onExpired);
  }, [clear]);

  return <AuthContext.Provider value={{ loading, hosted, backend, session, currentSessionId, context, bootstrapError, refresh, clear }}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthState {
  const value = useOptionalAuth();
  if (!value) throw new Error("useAuth must be used within AuthProvider");
  return value;
}

/** Shared presentation components can also render in the standalone gallery. */
export function useOptionalAuth(): AuthState | null {
  return useContext(AuthContext);
}
