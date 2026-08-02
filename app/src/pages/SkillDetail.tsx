import { useEffect, useState } from "react";
import { useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import CodeMirror from "@uiw/react-codemirror";
import { Plus, X, RotateCcw, GitCompare, BadgeCheck } from "lucide-react";
import { toast } from "sonner";
import { api, encodeContent, type Commit, type FileDiff } from "../lib/api";
import { useAuth } from "../lib/auth";
import { useT } from "../lib/i18n";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { Input } from "@/components/ui/input";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { DiffBlock } from "@/components/DiffBlock";
import { FeedbackPanel } from "@/components/FeedbackPanel";

export function SkillDetail() {
  const { id } = useParams({ from: "/skill/$id" });
  const qc = useQueryClient();
  const { canWrite, user } = useAuth();
  const { t } = useT();

  const skill = useQuery({ queryKey: ["skill", id], queryFn: () => api.getSkill(id) });
  const commits = useQuery({ queryKey: ["commits", id], queryFn: () => api.listCommits(id) });
  const head = commits.data?.[0]?.id;

  const [files, setFiles] = useState<Record<string, string>>({});
  const [active, setActive] = useState<string | null>(null);
  const [loadedFor, setLoadedFor] = useState<string | null>(null);

  useEffect(() => {
    if (!commits.isSuccess || loadedFor === (head ?? "")) return;
    (async () => {
      const next: Record<string, string> = {};
      if (head) {
        for (const e of await api.getTree(head)) next[e.path] = await api.getFile(head, e.path);
      }
      setFiles(next);
      setActive(Object.keys(next)[0] ?? null);
      setLoadedFor(head ?? "");
    })();
  }, [commits.isSuccess, head, loadedFor]);

  const [author, setAuthor] = useState("");
  const [message, setMessage] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [newPath, setNewPath] = useState("");

  // Pre-fill the commit author with the signed-in username.
  useEffect(() => {
    if (user?.username) setAuthor((a) => a || user.username);
  }, [user]);

  const reload = () => {
    setLoadedFor(null);
    qc.invalidateQueries({ queryKey: ["commits", id] });
    qc.invalidateQueries({ queryKey: ["skill", id] });
  };

  const payload = () =>
    Object.entries(files).map(([path, content]) => ({ path, content: encodeContent(content) }));

  const commit = useMutation({
    mutationFn: () => api.commit(id, { author: author.trim(), message: message.trim(), files: payload() }),
    onSuccess: () => {
      setMessage("");
      reload();
      toast.success(t("detail.committed"));
    },
    onError: (e) => toast.error(t("detail.commitRejected", { error: (e as Error).message })),
  });

  const rollback = useMutation({
    mutationFn: (cid: string) => api.rollback(id, { to_commit: cid, author: author || "anon" }),
    onSuccess: reload,
  });

  async function validateNow() {
    try {
      const res = await api.validate(payload());
      if (res.ok) {
        toast.success(t("detail.validOk", { name: res.meta?.name ?? "" }));
        for (const w of res.warnings ?? []) toast.warning(w);
      } else {
        toast.error(t("detail.invalid", { error: res.error ?? "" }));
      }
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  const [diff, setDiff] = useState<{ cid: string; entries: FileDiff[] } | null>(null);
  async function showDiff(c: Commit) {
    setDiff({ cid: c.id, entries: c.parent ? await api.diff(c.parent, c.id) : [] });
  }

  function confirmAdd() {
    const p = newPath.trim();
    setFiles((f) => ({ ...f, [p]: "" }));
    setActive(p);
    setNewPath("");
    setAddOpen(false);
  }
  function removeFile(path: string) {
    setFiles((f) => {
      const n = { ...f };
      delete n[path];
      return n;
    });
    if (active === path) setActive(null);
  }

  if (skill.error)
    return (
      <p className="text-sm text-muted-foreground">
        {t("detail.notFound", { error: (skill.error as Error).message })}
      </p>
    );

  const s = skill.data;
  const paths = Object.keys(files).sort();

  const pathErr = ((): string | null => {
    const p = newPath.trim();
    if (!p) return null; // don't show an error until they type
    if (p.startsWith("/")) return t("detail.errLeadingSlash");
    if (p.includes("..")) return t("detail.errDotDot");
    if (p.endsWith("/")) return t("detail.errFileName");
    if (files[p] !== undefined) return t("detail.errExists");
    return null;
  })();
  const meta = s?.metadata ?? {};
  const hasManifest = !!(s && (s.license || s.compatibility || s.allowed_tools || Object.keys(meta).length));

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="flex items-center gap-2 text-2xl font-semibold">
          {s?.name}
          {s && (
            <Badge className="font-mono" variant={s.kind === "prompt" ? "outline" : "secondary"}>
              {s.kind}
            </Badge>
          )}
        </h1>
        {s?.description && <p className="text-sm text-muted-foreground">{s.description}</p>}
      </div>

      {hasManifest && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("detail.manifest")}</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-2 text-sm">
            {s?.license && (
              <div>
                <span className="label-mono">{t("detail.license")}</span> {s.license}
              </div>
            )}
            {s?.compatibility && (
              <div>
                <span className="label-mono">{t("detail.compatibility")}</span> {s.compatibility}
              </div>
            )}
            {s?.allowed_tools && (
              <div>
                <span className="label-mono">{t("detail.allowedTools")}</span>{" "}
                <code className="rounded bg-muted px-1 font-mono">{s.allowed_tools}</code>
              </div>
            )}
            {Object.keys(meta).length > 0 && (
              <div className="flex flex-wrap gap-1">
                {Object.entries(meta).map(([k, v]) => (
                  <Badge key={k} className="font-mono" variant="outline">
                    {k}: {v}
                  </Badge>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      )}

      <Card>
        <CardContent className="pt-6">
          <div className="flex items-start gap-4">
            <div className="flex w-52 flex-col gap-1">
              <div className="flex items-center">
                <span className="label-mono">{t("detail.files")}</span>
                <div className="flex-1" />
                {canWrite && (
                  <Button variant="ghost" size="sm" onClick={() => setAddOpen(true)}>
                    <Plus data-icon="inline-start" />
                    {t("detail.newFile")}
                  </Button>
                )}
              </div>
              {paths.map((p) => (
                <div key={p} className="flex items-center gap-1">
                  <button
                    onClick={() => setActive(p)}
                    className={cn(
                      "truncate text-left font-mono text-sm hover:underline",
                      p === active ? "font-semibold" : "text-muted-foreground",
                    )}
                  >
                    {p}
                  </button>
                  <div className="flex-1" />
                  {canWrite && (
                    <button onClick={() => removeFile(p)} className="text-muted-foreground hover:text-destructive">
                      <X className="size-4" />
                    </button>
                  )}
                </div>
              ))}
              {paths.length === 0 && (
                <span className="text-sm text-muted-foreground">{t("detail.emptyFiles")}</span>
              )}
            </div>

            <div className="min-w-0 flex-1">
              {active ? (
                <div className="overflow-hidden rounded-md border">
                  <CodeMirror
                    value={files[active] ?? ""}
                    height="320px"
                    editable={canWrite}
                    readOnly={!canWrite}
                    onChange={(v) => setFiles((f) => ({ ...f, [active]: v }))}
                  />
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">{t("detail.selectFile")}</p>
              )}
            </div>
          </div>

          <Separator className="my-4" />

          <div className="flex items-center gap-2">
            {canWrite && (
              <>
                <Input
                  className="w-40"
                  placeholder={t("detail.author")}
                  value={author}
                  onChange={(e) => setAuthor(e.target.value)}
                />
                <Input
                  className="flex-1"
                  placeholder={t("detail.commitMessage")}
                  value={message}
                  onChange={(e) => setMessage(e.target.value)}
                />
              </>
            )}
            {!canWrite && <div className="flex-1" />}
            {s?.kind === "skill" && (
              <Button variant="outline" onClick={validateNow} disabled={paths.length === 0}>
                <BadgeCheck data-icon="inline-start" />
                {t("detail.validate")}
              </Button>
            )}
            {canWrite && (
              <Button
                disabled={paths.length === 0 || !author.trim() || !message.trim() || commit.isPending}
                onClick={() => commit.mutate()}
              >
                {t("detail.commit")}
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      {s && <FeedbackPanel skillId={id} isSkill={s.kind === "skill"} />}

      <h2 className="text-lg font-semibold">{t("detail.history")}</h2>
      {commits.data?.map((c) => (
        <Card key={c.id}>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-sm font-normal">
              <code className="rounded bg-muted px-1 font-mono">{c.id.slice(0, 8)}</code>
              <span className="font-medium">{c.message}</span>
              <span className="label-mono">{c.author}</span>
              <div className="flex-1" />
              <Button variant="ghost" size="sm" onClick={() => showDiff(c)}>
                <GitCompare data-icon="inline-start" />
                {t("detail.diff")}
              </Button>
              {canWrite && (
                <Button variant="ghost" size="sm" onClick={() => rollback.mutate(c.id)}>
                  <RotateCcw data-icon="inline-start" />
                  {t("detail.rollback")}
                </Button>
              )}
            </CardTitle>
          </CardHeader>
          {diff?.cid === c.id && (
            <CardContent>
              {diff.entries.length === 0 && (
                <div className="text-sm text-muted-foreground">{t("detail.initialCommit")}</div>
              )}
              {diff.entries
                .filter((f) => f.status !== "unchanged")
                .map((f) => (
                  <div key={f.path} className="mb-2">
                    <div className="font-mono text-xs text-muted-foreground">
                      {f.status} — {f.path}
                    </div>
                    {f.diff && <DiffBlock text={f.diff} />}
                  </div>
                ))}
            </CardContent>
          )}
        </Card>
      ))}

      <Dialog open={addOpen} onOpenChange={setAddOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("detail.newFileTitle")}</DialogTitle>
          </DialogHeader>
          <Field data-invalid={!!pathErr}>
            <FieldLabel htmlFor="newpath">{t("detail.path")}</FieldLabel>
            <Input
              id="newpath"
              autoFocus
              className="font-mono"
              value={newPath}
              onChange={(e) => setNewPath(e.target.value)}
              placeholder={t("detail.pathPlaceholder")}
              aria-invalid={!!pathErr}
              onKeyDown={(e) => {
                if (e.key === "Enter" && newPath.trim() && !pathErr) confirmAdd();
              }}
            />
            <FieldDescription>{t("detail.pathHint")}</FieldDescription>
            {pathErr && <FieldError>{pathErr}</FieldError>}
          </Field>
          <DialogFooter>
            <Button disabled={!newPath.trim() || !!pathErr} onClick={confirmAdd}>
              {t("detail.add")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
