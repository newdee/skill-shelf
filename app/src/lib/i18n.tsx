/* Lightweight zh/en i18n: no deps, dictionary modules per domain.
 * UI chrome only — user data (skills, config values) is never translated. */
import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { dicts } from "./i18n/dicts";

export type Lang = "zh" | "en";

const LANG_KEY = "skillshelf.lang";

function initialLang(): Lang {
  const saved = localStorage.getItem(LANG_KEY);
  if (saved === "zh" || saved === "en") return saved;
  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

interface I18n {
  lang: Lang;
  setLang: (l: Lang) => void;
  /** Translate a dict key; `{x}` placeholders fill from vars. Unknown keys echo back. */
  t: (key: string, vars?: Record<string, string | number>) => string;
}

const Ctx = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(initialLang);
  const setLang = useCallback((l: Lang) => {
    localStorage.setItem(LANG_KEY, l);
    setLangState(l);
  }, []);
  const t = useCallback(
    (key: string, vars?: Record<string, string | number>) => {
      const entry = dicts[key];
      let s = entry ? entry[lang] : key;
      if (vars) for (const [k, v] of Object.entries(vars)) s = s.replaceAll(`{${k}}`, String(v));
      return s;
    },
    [lang],
  );
  const value = useMemo(() => ({ lang, setLang, t }), [lang, setLang, t]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useT(): I18n {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useT must be used inside I18nProvider");
  return ctx;
}
