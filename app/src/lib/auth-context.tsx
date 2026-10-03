import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
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
  loading: boolean;
  hosted: boolean;
  session: SessionUser | null;
  currentSessionId: string | null;
  context: SessionContext | null;
  refresh: () => Promise<void>;
  clear: () => void;
};

const AuthContext = createContext<AuthState | null>(null);

async function currentSession(): Promise<{ user: SessionUser; sessionId: string | null } | null> {
  const result = await authClient.getSession();
  if (!result.data?.user) return null;
  return {
    user: result.data.user as SessionUser,
    sessionId: typeof result.data.session?.id === "string" ? result.data.session.id : null,
  };
}

export function AuthProvider({ hosted, children }: { hosted: boolean; children: ReactNode }) {
  const [loading, setLoading] = useState(hosted);
  const [session, setSession] = useState<SessionUser | null>(null);
  const [currentSessionId, setCurrentSessionId] = useState<string | null>(null);
  const [context, setContext] = useState<SessionContext | null>(null);

  const clear = useCallback(() => {
    setSession(null);
    setCurrentSessionId(null);
    setContext(null);
  }, []);

  const refresh = useCallback(async () => {
    if (!hosted) {
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const current = await currentSession();
      const user = current?.user ?? null;
      setSession(user);
      setCurrentSessionId(current?.sessionId ?? null);
      if (!user || !user.emailVerified || !user.twoFactorEnabled) {
        setContext(null);
        return;
      }
      const value = await request<SessionContext & { ok: true }>("/api/me");
      setContext({ user: value.user, workspaceRole: value.workspaceRole, companies: value.companies });
    } catch (error) {
      if (!(error instanceof ApiError) || error.code === "unauthorized") clear();
      else clear();
    } finally {
      setLoading(false);
    }
  }, [clear, hosted]);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    const onExpired = () => clear();
    window.addEventListener("rentemester:auth-expired", onExpired);
    return () => window.removeEventListener("rentemester:auth-expired", onExpired);
  }, [clear]);

  return <AuthContext.Provider value={{ loading, hosted, session, currentSessionId, context, refresh, clear }}>{children}</AuthContext.Provider>;
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
