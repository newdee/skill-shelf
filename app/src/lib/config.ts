// Backend base URL is configured at runtime (not baked in at build time), so
// the same build can point at any server. Priority: user setting (localStorage)
// > desktop sidecar default (when running in Tauri) > VITE_API_BASE > fallback.
const KEY = "skillshelf.apiBase";
const WEB_FALLBACK = "http://127.0.0.1:8080";
// Must match SIDECAR_PORT in src-tauri/src/lib.rs.
const DESKTOP_SIDECAR = "http://127.0.0.1:8765";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function getApiBase(): string {
  const saved = localStorage.getItem(KEY);
  if (saved) return saved;
  if (isTauri()) return DESKTOP_SIDECAR;
  return (import.meta.env.VITE_API_BASE as string | undefined) || WEB_FALLBACK;
}

export function setApiBase(value: string): void {
  localStorage.setItem(KEY, value.trim().replace(/\/+$/, ""));
}
