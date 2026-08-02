import { useSyncExternalStore } from "react";

export type Theme = "system" | "light" | "dark";
const KEY = "skillshelf.theme";

export function getTheme(): Theme {
  return (localStorage.getItem(KEY) as Theme) || "system";
}

function resolve(t: Theme): "light" | "dark" {
  if (t === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return t;
}

export function applyTheme(t: Theme = getTheme()): void {
  document.documentElement.classList.toggle("dark", resolve(t) === "dark");
}

export function setTheme(t: Theme): void {
  localStorage.setItem(KEY, t);
  applyTheme(t);
}

/** Reactive resolved theme, for widgets that can't inherit CSS variables
 * (e.g. CodeMirror). Tracks the `dark` class applyTheme() toggles. */
export function useIsDark(): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const obs = new MutationObserver(onChange);
      obs.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
      return () => obs.disconnect();
    },
    () => document.documentElement.classList.contains("dark"),
  );
}

/** Apply on startup and keep following the system when set to "system". */
export function initTheme(): void {
  applyTheme();
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", () => {
      if (getTheme() === "system") applyTheme("system");
    });
}
