# .env Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import a `.env` file (paste or file picker) into the selected config-center namespace's draft, with a preview of added/overwritten/skipped/invalid entries.

**Architecture:** Frontend-only. A pure parser (`parseDotenv`) and classifier (`classifyImport`) in `app/src/lib/dotenv.ts`, a new `ImportEnvDialog` component, wired into `ConfigCenter.tsx` next to the Publish button. Submits via the existing `api.putNamespace`. Spec: `docs/superpowers/specs/2026-08-02-env-import-design.md`.

**Tech Stack:** React 19, TypeScript, shadcn/ui, TanStack Query, vitest (new devDep, unit tests only).

## Global Constraints

- All imported values are strings — no type inference.
- Import merges into the DRAFT only; never publishes, never deletes keys (patch contains no nulls).
- UI copy in English, matching the existing ConfigCenter page ("Import .env", "Import N keys to draft").
- Reuse existing shadcn components and tokens; no new styles, no new theme, no added animation.
- Never mark a task complete with failing tests. Run commands from `app/`.

---

### Task 1: vitest setup + `parseDotenv`

**Files:**
- Modify: `app/package.json` (add vitest devDep + `test` script)
- Create: `app/src/lib/dotenv.ts`
- Test: `app/src/lib/dotenv.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces:
  ```ts
  export interface DotenvWarning { line: number; text: string; reason: string }
  export interface DotenvParseResult { vars: Record<string, string>; warnings: DotenvWarning[] }
  export function parseDotenv(text: string): DotenvParseResult
  ```

- [ ] **Step 1: Install vitest and add the test script**

```bash
cd app && npm install -D vitest
```

In `app/package.json` scripts, add: `"test": "vitest run"`.

- [ ] **Step 2: Write the failing tests**

`app/src/lib/dotenv.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { parseDotenv } from "./dotenv";

describe("parseDotenv", () => {
  it("parses basic KEY=value lines", () => {
    const r = parseDotenv("A=1\nDB_URL=postgres://x\n");
    expect(r.vars).toEqual({ A: "1", DB_URL: "postgres://x" });
    expect(r.warnings).toEqual([]);
  });

  it("strips export prefix", () => {
    expect(parseDotenv("export FOO=bar").vars).toEqual({ FOO: "bar" });
  });

  it("skips comments and blank lines", () => {
    const r = parseDotenv("# top\n\n  \nA=1\n");
    expect(r.vars).toEqual({ A: "1" });
    expect(r.warnings).toEqual([]);
  });

  it("truncates unquoted values at an inline comment", () => {
    expect(parseDotenv("A=hello # world").vars).toEqual({ A: "hello" });
  });

  it("keeps # inside quoted values", () => {
    expect(parseDotenv('A="hello # world"').vars).toEqual({ A: "hello # world" });
  });

  it("single quotes are literal (no escapes)", () => {
    expect(parseDotenv("A='line\\nnot-newline'").vars).toEqual({ A: "line\\nnot-newline" });
  });

  it("double quotes support escapes", () => {
    expect(parseDotenv('A="a\\nb\\tc\\"d\\\\e"').vars).toEqual({ A: 'a\nb\tc"d\\e' });
  });

  it("trims whitespace around unquoted values", () => {
    expect(parseDotenv("A=  padded  ").vars).toEqual({ A: "padded" });
  });

  it("later duplicate keys win", () => {
    expect(parseDotenv("A=1\nA=2").vars).toEqual({ A: "2" });
  });

  it("warns on a line without '='", () => {
    const r = parseDotenv("JUSTAWORD");
    expect(r.vars).toEqual({});
    expect(r.warnings).toEqual([{ line: 1, text: "JUSTAWORD", reason: "missing '='" }]);
  });

  it("warns on an invalid key", () => {
    const r = parseDotenv("2BAD=x\nGOOD=y");
    expect(r.vars).toEqual({ GOOD: "y" });
    expect(r.warnings[0]).toMatchObject({ line: 1, reason: "invalid key" });
  });

  it("warns on an unterminated quote", () => {
    const r = parseDotenv('A="oops');
    expect(r.vars).toEqual({});
    expect(r.warnings[0]).toMatchObject({ line: 1, reason: "unterminated quote" });
  });

  it("empty and comment-only input yields nothing", () => {
    expect(parseDotenv("")).toEqual({ vars: {}, warnings: [] });
    expect(parseDotenv("# only\n# comments\n")).toEqual({ vars: {}, warnings: [] });
  });

  it("allows an empty value", () => {
    expect(parseDotenv("A=").vars).toEqual({ A: "" });
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd app && npx vitest run src/lib/dotenv.test.ts`
Expected: FAIL — `Cannot find module './dotenv'` (or equivalent).

- [ ] **Step 4: Implement `parseDotenv`**

`app/src/lib/dotenv.ts`:

```ts
// Client-side .env parser for importing into a config-center namespace.
// All values are strings by design — see the 2026-08-02 spec.

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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd app && npx vitest run src/lib/dotenv.test.ts`
Expected: PASS (14 tests).

- [ ] **Step 6: Commit**

```bash
git add app/package.json app/package-lock.json app/src/lib/dotenv.ts app/src/lib/dotenv.test.ts
git commit -m "Add client-side .env parser with vitest setup"
```

---

### Task 2: `classifyImport`

**Files:**
- Modify: `app/src/lib/dotenv.ts` (append)
- Test: `app/src/lib/dotenv.test.ts` (append)

**Interfaces:**
- Consumes: `parseDotenv` output (`vars: Record<string, string>`).
- Produces:
  ```ts
  export type ImportStatus = "added" | "overwritten" | "skipped";
  export interface ImportEntry {
    key: string;
    value: string;        // the incoming .env value
    status: ImportStatus;
    old?: unknown;        // current draft value (absent when added or masked)
    oldMasked?: boolean;  // true when the draft value is secret/hidden
  }
  // draft: NamespaceView.vars-shaped objects ({ key, value?, secret })
  export function classifyImport(
    vars: Record<string, string>,
    draft: { key: string; value?: unknown; secret: boolean }[],
  ): ImportEntry[]
  ```
  Rules (from the spec): key not in draft → `added`; in draft with the identical
  string value → `skipped`; anything else (different string, non-string draft
  value, masked secret) → `overwritten`. Output keeps `Object.entries` order.

- [ ] **Step 1: Write the failing tests (append to `dotenv.test.ts`)**

```ts
import { classifyImport } from "./dotenv";

describe("classifyImport", () => {
  const draft = [
    { key: "SAME", value: "x", secret: false },
    { key: "DIFF", value: "old", secret: false },
    { key: "NUM", value: 5, secret: false },
    { key: "SEC", value: undefined, secret: true },
  ];

  it("classifies added / overwritten / skipped", () => {
    const r = classifyImport({ NEW: "1", SAME: "x", DIFF: "new" }, draft);
    expect(r).toEqual([
      { key: "NEW", value: "1", status: "added" },
      { key: "SAME", value: "x", status: "skipped", old: "x" },
      { key: "DIFF", value: "new", status: "overwritten", old: "old" },
    ]);
  });

  it("non-string draft values are overwritten, never skipped", () => {
    expect(classifyImport({ NUM: "5" }, draft)).toEqual([
      { key: "NUM", value: "5", status: "overwritten", old: 5 },
    ]);
  });

  it("masked secrets are overwritten without exposing old value", () => {
    expect(classifyImport({ SEC: "s3cret" }, draft)).toEqual([
      { key: "SEC", value: "s3cret", status: "overwritten", oldMasked: true },
    ]);
  });

  it("empty draft classifies everything as added", () => {
    expect(classifyImport({ A: "1" }, [])).toEqual([{ key: "A", value: "1", status: "added" }]);
  });
});
```

- [ ] **Step 2: Run tests to verify the new ones fail**

Run: `cd app && npx vitest run src/lib/dotenv.test.ts`
Expected: FAIL — `classifyImport` not exported.

- [ ] **Step 3: Implement (append to `dotenv.ts`)**

```ts
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
```

Note `cur.value === value`: a strict `===` against a string is only true when the
draft value is that identical string, so the non-string rule falls out for free.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd app && npx vitest run src/lib/dotenv.test.ts`
Expected: PASS (18 tests).

- [ ] **Step 5: Commit**

```bash
git add app/src/lib/dotenv.ts app/src/lib/dotenv.test.ts
git commit -m "Add import classification against the namespace draft"
```

---

### Task 3: `ImportEnvDialog` + ConfigCenter wiring

**Files:**
- Create: `app/src/components/ImportEnvDialog.tsx`
- Modify: `app/src/pages/ConfigCenter.tsx` (import block ~L1-31; publish bar ~L284-300)

**Interfaces:**
- Consumes: `parseDotenv`, `classifyImport` from `@/lib/dotenv` (Task 1–2 signatures); `api.putNamespace(ns, patch)`; `NamespaceView` from `../lib/api`; shadcn `Dialog/Button/Badge/Textarea/Alert`.
- Produces: `export function ImportEnvDialog(props: { ns: string; draft: ConfigVar[]; open: boolean; onOpenChange: (o: boolean) => void; onImported: () => void })`

- [ ] **Step 1: Write the component**

`app/src/components/ImportEnvDialog.tsx`:

```tsx
import { useMemo, useRef, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { api, type ConfigVar } from "../lib/api";
import { classifyImport, parseDotenv } from "../lib/dotenv";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";
import { FileUp } from "lucide-react";

interface Props {
  ns: string;
  draft: ConfigVar[];
  open: boolean;
  onOpenChange: (o: boolean) => void;
  /** Called after a successful import so the page can invalidate its queries. */
  onImported: () => void;
}

export function ImportEnvDialog({ ns, draft, open, onOpenChange, onImported }: Props) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const { entries, warnings } = useMemo(() => {
    const parsed = parseDotenv(text);
    return { entries: classifyImport(parsed.vars, draft), warnings: parsed.warnings };
  }, [text, draft]);

  const added = entries.filter((e) => e.status === "added");
  const overwritten = entries.filter((e) => e.status === "overwritten");
  const skipped = entries.filter((e) => e.status === "skipped");
  const toImport = added.length + overwritten.length;

  const importDraft = useMutation({
    mutationFn: () => {
      const patch: Record<string, unknown> = {};
      for (const e of entries) if (e.status !== "skipped") patch[e.key] = e.value;
      return api.putNamespace(ns, patch);
    },
    onSuccess: () => {
      onImported();
      close(false);
      toast.success(`Merged ${toImport} keys into the ${ns} draft — Publish to go live`);
    },
    onError: (e) => setError((e as Error).message),
  });

  const close = (o: boolean) => {
    onOpenChange(o);
    if (!o) {
      setText("");
      setError(null);
      if (fileRef.current) fileRef.current.value = "";
    }
  };

  const pickFile = (f: File | undefined) => {
    if (!f) return;
    const reader = new FileReader();
    reader.onload = () => setText(String(reader.result ?? ""));
    reader.onerror = () => toast.error("Couldn't read the file");
    reader.readAsText(f);
  };

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Import .env → {ns}</DialogTitle>
          <DialogDescription>
            Values are imported as strings and merged into the draft. Consumers are unaffected until
            you Publish. Existing keys not present below are left untouched.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <Textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={"KEY=value\n# comments are ignored"}
            spellCheck={false}
            className="min-h-32 font-mono text-sm"
            aria-label=".env content"
          />
          <div>
            <input
              ref={fileRef}
              type="file"
              className="hidden"
              onChange={(e) => pickFile(e.target.files?.[0])}
            />
            <Button variant="ghost" size="sm" onClick={() => fileRef.current?.click()}>
              <FileUp /> Choose file
            </Button>
          </div>

          {text.trim() ? (
            <div className="flex flex-col gap-2">
              <div className="flex flex-wrap items-center gap-2 text-sm">
                <Badge variant="secondary">{added.length} added</Badge>
                <Badge
                  variant="outline"
                  className={overwritten.length ? "border-amber-500/50 text-amber-600 dark:text-amber-400" : ""}
                >
                  {overwritten.length} overwritten
                </Badge>
                <Badge variant="outline">{skipped.length} skipped (same value)</Badge>
                {warnings.length > 0 && <Badge variant="destructive">{warnings.length} invalid</Badge>}
              </div>
              {entries.length > 0 && (
                <div className="flex max-h-48 flex-col gap-1 overflow-y-auto rounded-lg border border-border/60 p-2 text-sm">
                  {entries.map((e) => (
                    <div key={e.key} className="flex items-center gap-2">
                      <Badge
                        variant="outline"
                        className={
                          "w-24 justify-center text-xs " +
                          (e.status === "overwritten"
                            ? "border-amber-500/50 text-amber-600 dark:text-amber-400"
                            : e.status === "skipped"
                              ? "opacity-60"
                              : "")
                        }
                      >
                        {e.status}
                      </Badge>
                      <code className="rounded bg-muted px-1">{e.key}</code>
                      <span className="truncate text-muted-foreground">
                        {e.status === "overwritten" && (
                          <>
                            <s>{e.oldMasked ? "••••••••" : JSON.stringify(e.old)}</s>
                            {" → "}
                          </>
                        )}
                        {JSON.stringify(e.value)}
                      </span>
                    </div>
                  ))}
                </div>
              )}
              {warnings.length > 0 && (
                <Alert variant="destructive">
                  <AlertTitle>Some lines can't be imported</AlertTitle>
                  <AlertDescription>
                    <ul className="list-inside list-disc">
                      {warnings.map((w) => (
                        <li key={w.line}>
                          line {w.line}: {w.reason} — <code>{w.text.trim()}</code>
                        </li>
                      ))}
                    </ul>
                  </AlertDescription>
                </Alert>
              )}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              Paste .env content above or choose a file to see a preview.
            </p>
          )}

          {error && (
            <Alert variant="destructive">
              <AlertTitle>Import failed</AlertTitle>
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => close(false)}>
            Cancel
          </Button>
          <Button disabled={toImport === 0 || importDraft.isPending} onClick={() => importDraft.mutate()}>
            {importDraft.isPending ? "Importing…" : `Import ${toImport} keys to draft`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Wire into ConfigCenter**

In `app/src/pages/ConfigCenter.tsx`:

1. Add import: `import { ImportEnvDialog } from "../components/ImportEnvDialog";`
2. Add state next to the other dialog state (~L95): `const [importOpen, setImportOpen] = useState(false);`
3. In the publish bar (between the History and Publish buttons, ~L294):

```tsx
<Button size="sm" variant="outline" onClick={() => setImportOpen(true)}>
  Import .env
</Button>
```

4. Render the dialog next to the other dialogs (before `{/* Publish dialog */}`):

```tsx
<ImportEnvDialog
  ns={sel}
  draft={nsView?.vars ?? []}
  open={importOpen}
  onOpenChange={setImportOpen}
  onImported={() => invalidateNs(sel)}
/>
```

5. Reset on namespace switch — extend the existing `useEffect` at ~L203:

```tsx
useEffect(() => {
  setHistOpen(false);
  setViewVer(null);
  setImportOpen(false);
}, [sel]);
```

- [ ] **Step 3: Typecheck + lint + tests**

Run: `cd app && npx tsc -b && npm run lint && npm test`
Expected: all clean, 18 tests pass.

- [ ] **Step 4: Manual smoke test**

Start the stack (server + `npm run dev`), open Config center as admin, and verify:
- "Import .env" button appears next to Publish.
- Pasting `A=1\nA=1 # dup` style content previews counts live.
- A file picked via "Choose file" fills the textarea.
- Import merges into the draft (unpublished-changes bar appears), toast shows, dialog closes.
- Import button disabled when everything is skipped/invalid/empty.

- [ ] **Step 5: Commit**

```bash
git add app/src/components/ImportEnvDialog.tsx app/src/pages/ConfigCenter.tsx
git commit -m "Add .env import dialog to the config center"
```

---

### Task 4: Final verification

- [ ] **Step 1: Full frontend check**

Run: `cd app && npx tsc -b && npm run lint && npm test`
Expected: clean.

- [ ] **Step 2: Server tests untouched but still green**

Run: `cargo test --workspace`
Expected: PASS (no server changes; guard against accidental edits).

- [ ] **Step 3: Check off plan checkboxes, update docs if drifted, final review**

Request code review of the whole branch diff before declaring done.
