// Client-side .env parser for importing into a config-center namespace.
// All values are strings by design — see docs/superpowers/specs/2026-08-02-env-import-design.md.

export interface DotenvWarning {
  line: number;
  text: string;
  reason: string;
}

export interface DotenvParseResult {
  vars: Record<string, string>;
  warnings: DotenvWarning[];
}

const KEY_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;

export function parseDotenv(text: string): DotenvParseResult {
  const vars: Record<string, string> = {};
  const warnings: DotenvWarning[] = [];
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const warn = (reason: string) => warnings.push({ line: i + 1, text: raw, reason });
    const body = line.startsWith("export ") ? line.slice("export ".length).trimStart() : line;
    const eq = body.indexOf("=");
    if (eq === -1) return warn("missing '='");
    const key = body.slice(0, eq).trim();
    if (!KEY_RE.test(key)) return warn("invalid key");
    const rest = body.slice(eq + 1).trim();
    let value: string;
    if (rest.startsWith('"')) {
      const m = /^"((?:\\.|[^"\\])*)"/.exec(rest);
      if (!m) return warn("unterminated quote");
      value = m[1].replace(/\\(.)/g, (_, c: string) =>
        c === "n" ? "\n" : c === "t" ? "\t" : c === "r" ? "\r" : c,
      );
    } else if (rest.startsWith("'")) {
      const end = rest.indexOf("'", 1);
      if (end === -1) return warn("unterminated quote");
      value = rest.slice(1, end);
    } else {
      const hash = rest.search(/\s#/);
      value = (hash === -1 ? rest : rest.slice(0, hash)).trim();
    }
    vars[key] = value;
  });
  return { vars, warnings };
}

export type ImportStatus = "added" | "overwritten" | "skipped";

export interface ImportEntry {
  key: string;
  value: string;
  status: ImportStatus;
  old?: unknown;
  oldMasked?: boolean;
}

/** Classify parsed .env vars against the namespace draft (spec §2 分类定义). */
export function classifyImport(
  vars: Record<string, string>,
  draft: { key: string; value?: unknown; secret: boolean }[],
): ImportEntry[] {
  const byKey = new Map(draft.map((v) => [v.key, v]));
  return Object.entries(vars).map(([key, value]) => {
    const cur = byKey.get(key);
    if (!cur) return { key, value, status: "added" as const };
    if (cur.secret || cur.value === undefined)
      return { key, value, status: "overwritten" as const, oldMasked: true };
    if (cur.value === value) return { key, value, status: "skipped" as const, old: cur.value };
    return { key, value, status: "overwritten" as const, old: cur.value };
  });
}
