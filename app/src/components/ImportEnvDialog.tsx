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

  const close = (o: boolean) => {
    onOpenChange(o);
    if (!o) {
      setText("");
      setError(null);
      if (fileRef.current) fileRef.current.value = "";
    }
  };

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
