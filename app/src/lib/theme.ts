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

/** Apply on startup and keep following the system when set to "system". */
export function initTheme(): void {
  applyTheme();
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", () => {
      if (getTheme() === "system") applyTheme("system");
    });
}
