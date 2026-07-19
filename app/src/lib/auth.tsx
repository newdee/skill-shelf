import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { api, type AuthResponse } from "./api";

interface AuthUser {
  username: string;
  role: string;
}

interface AuthState {
  /** Whether the backend enforces auth (admin-only writes). */
  authEnabled: boolean;
  user: AuthUser | null;
  /** Can the current context perform authoring/delete actions? */
  canWrite: boolean;
  signIn: (username: string, password: string) => Promise<void>;
  signUp: (username: string, password: string) => Promise<void>;
  signOut: () => void;
}

const TOKEN_KEY = "skillshelf.token";
const USER_KEY = "skillshelf.user";
const Ctx = createContext<AuthState | null>(null);

function loadUser(): AuthUser | null {
  const s = localStorage.getItem(USER_KEY);
  return s ? (JSON.parse(s) as AuthUser) : null;
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [authEnabled, setAuthEnabled] = useState(false);
  const [user, setUser] = useState<AuthUser | null>(loadUser());

  useEffect(() => {
    api
      .status()
      .then((s) => setAuthEnabled(!!s.auth_enabled))
      .catch(() => setAuthEnabled(false));
  }, []);

  const apply = (r: AuthResponse) => {
    localStorage.setItem(TOKEN_KEY, r.token);
    const u = { username: r.username, role: r.role };
    localStorage.setItem(USER_KEY, JSON.stringify(u));
    setUser(u);
  };

  const value: AuthState = {
    authEnabled,
    user,
    // Open instance (no auth) → anyone can write; otherwise admins only.
    canWrite: !authEnabled || user?.role === "admin",
    signIn: async (username, password) => apply(await api.signin({ username, password })),
    signUp: async (username, password) => apply(await api.signup({ username, password })),
    signOut: () => {
      localStorage.removeItem(TOKEN_KEY);
      localStorage.removeItem(USER_KEY);
      setUser(null);
    },
  };

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useAuth(): AuthState {
  const v = useContext(Ctx);
  if (!v) throw new Error("useAuth must be used within AuthProvider");
  return v;
}
