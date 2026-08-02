import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { api, type NamespaceInfo, type NewClient } from "../lib/api";
import { useAuth } from "../lib/auth";
import { getApiBase } from "../lib/config";
import { useT } from "../lib/i18n";
import { ImportEnvDialog } from "../components/ImportEnvDialog";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Copy, FileUp, Trash2 } from "lucide-react";

const GLOBAL_NS = "_global";

/** Parse an input as JSON if it's a valid object/number/bool, else a string. */
function parseValue(raw: string): unknown {
  try {
    const p = JSON.parse(raw);
    return typeof p === "object" || typeof p === "number" || typeof p === "boolean" ? p : raw;
  } catch {
    return raw;
  }
}

/** Same rule as the backend `valid_namespace`. Returns an i18n key, or null when valid. */
function nsError(name: string): string | null {
  const n = name.trim();
  if (!n) return "config.nsErr.required";
  if (n.length > 128) return "config.nsErr.maxLen";
  if (n.includes("..")) return "config.nsErr.dots";
  if (!/^[a-zA-Z0-9._/-]+$/.test(n)) return "config.nsErr.charset";
  return null;
}

/** Format a unix-seconds timestamp; 0 (imported) shows as em dash. */
function fmtTime(sec: number): string {
  return sec ? new Date(sec * 1000).toLocaleString() : "—";
}

export function ConfigCenter() {
  const qc = useQueryClient();
  const { t } = useT();
  const { canWrite, user } = useAuth();
  // Admin-only queries: don't fire (and don't retry) for non-admins, so a
  // direct visit to /config doesn't storm the server with 401s.
  const nsQuery = useQuery({
    queryKey: ["namespaces"],
    queryFn: api.listNamespaces,
    enabled: canWrite,
    retry: false,
  });
  const namespaces = nsQuery.data;
  const { data: clients, isError: clientsError } = useQuery({
    queryKey: ["clients"],
    queryFn: api.listClients,
    enabled: canWrite,
    retry: false,
  });

  const [sel, setSel] = useState(GLOBAL_NS);
  const { data: nsView } = useQuery({
    queryKey: ["namespace", sel],
    queryFn: () => api.getNamespace(sel),
    enabled: canWrite,
    retry: false,
  });
  // Field-level draft-vs-published diff for the selected namespace.
  const { data: diff } = useQuery({
    queryKey: ["nsdiff", sel],
    queryFn: () => api.namespaceDiff(sel),
    enabled: canWrite,
    retry: false,
  });
  const [delKey, setDelKey] = useState<string | null>(null);
  // publish + history state
  const [pubOpen, setPubOpen] = useState(false);
  const [pubNote, setPubNote] = useState("");
  const [importOpen, setImportOpen] = useState(false);
  const [histOpen, setHistOpen] = useState(false);
  const [viewVer, setViewVer] = useState<number | null>(null);

  const { data: versions } = useQuery({
    queryKey: ["nsversions", sel],
    queryFn: () => api.namespaceVersions(sel),
    enabled: canWrite && histOpen,
    retry: false,
  });
  const { data: verVars } = useQuery({
    queryKey: ["nsversion", sel, viewVer],
    queryFn: () => api.namespaceVersion(sel, viewVer as number),
    enabled: canWrite && viewVer != null,
    retry: false,
  });

  const invalidateNs = (ns: string) => {
    qc.invalidateQueries({ queryKey: ["namespace", ns] });
    qc.invalidateQueries({ queryKey: ["nsdiff", ns] });
    qc.invalidateQueries({ queryKey: ["nsversions", ns] });
    qc.invalidateQueries({ queryKey: ["namespaces"] });
  };

  // new-key form for the selected namespace
  const [newKey, setNewKey] = useState("");
  const [newVal, setNewVal] = useState("");
  // new-namespace dialog
  const [nsOpen, setNsOpen] = useState(false);
  const [nsName, setNsName] = useState("");
  const [nsKey, setNsKey] = useState("");
  const [nsVal, setNsVal] = useState("");
  // new-client dialog + one-time token reveal
  const [clientOpen, setClientOpen] = useState(false);
  const [clientName, setClientName] = useState("");
  const [clientNs, setClientNs] = useState<Set<string>>(new Set());
  const [issued, setIssued] = useState<NewClient | null>(null);
  const [delClient, setDelClient] = useState<string | null>(null);

  const nsNames = useMemo(() => namespaces?.map((n) => n.name) ?? [GLOBAL_NS], [namespaces]);
  const serviceNs = nsNames.filter((n) => n !== GLOBAL_NS);

  const savePatch = useMutation({
    mutationFn: ({ ns, patch }: { ns: string; patch: Record<string, unknown> }) =>
      api.putNamespace(ns, patch),
    onSuccess: (_d, v) => {
      invalidateNs(v.ns);
      setNewKey("");
      setNewVal("");
    },
    onError: (e) => toast.error(t("config.toast.saveFailed", { msg: (e as Error).message })),
  });

  const publish = useMutation({
    mutationFn: () => api.publishNamespace(sel, { author: user?.username ?? "admin", note: pubNote.trim() }),
    onSuccess: () => {
      invalidateNs(sel);
      setPubOpen(false);
      setPubNote("");
      toast.success(t("config.toast.published"));
    },
    onError: (e) => toast.error(t("config.toast.publishFailed", { msg: (e as Error).message })),
  });

  const rollback = useMutation({
    mutationFn: (version: number) => api.rollbackNamespace(sel, version),
    onSuccess: () => {
      invalidateNs(sel);
      setHistOpen(false);
      setViewVer(null);
      toast.success(t("config.toast.rolledBack"));
    },
    onError: (e) => toast.error(t("config.toast.rollbackFailed", { msg: (e as Error).message })),
  });

  const createClient = useMutation({
    mutationFn: () =>
      api.createClient({ name: clientName.trim(), namespaces: [...clientNs] }),
    onSuccess: (c) => {
      qc.invalidateQueries({ queryKey: ["clients"] });
      setClientOpen(false);
      setClientName("");
      setClientNs(new Set());
      setIssued(c); // reveal the token exactly once
    },
    onError: (e) => toast.error(t("config.toast.createFailed", { msg: (e as Error).message })),
  });

  const removeClient = useMutation({
    mutationFn: (id: string) => api.deleteClient(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["clients"] });
      setDelClient(null);
      toast.success(t("config.toast.revoked"));
    },
    onError: (e) => toast.error(t("config.toast.revokeFailed", { msg: (e as Error).message })),
  });

  // keep the selected namespace valid if the list changes — but only once the
  // list has actually loaded, so a freshly-created (optimistically-seeded)
  // namespace isn't reset out from under the user mid-refetch.
  useEffect(() => {
    if (namespaces && !nsNames.includes(sel)) setSel(GLOBAL_NS);
  }, [namespaces, nsNames, sel]);

  // Reset version-history state when the selected namespace changes, so an open
  // History dialog never shows a previous namespace's version.
  useEffect(() => {
    setHistOpen(false);
    setViewVer(null);
    setImportOpen(false);
  }, [sel]);

  const newKeyErr = newKey.trim() && nsView?.vars.some((v) => v.key === newKey.trim())
    ? t("config.keyExists")
    : null;
  const nsNameErr = nsError(nsName);
  const resolveUrl = `${getApiBase()}/config/resolve?namespace=${sel}`;

  if (!canWrite) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("config.adminOnly")}</CardTitle>
          <CardDescription>{t("config.adminOnlyDesc")}</CardDescription>
        </CardHeader>
      </Card>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">{t("config.title")}</h1>
        <p className="mt-1 text-sm text-muted-foreground">{t("config.subtitle")}</p>
      </div>

      {(nsQuery.isError || clientsError) && (
        <div className="rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
          {t("config.loadError")}
        </div>
      )}

      {/* Namespaces + KV editor */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("config.namespaces")}</CardTitle>
          <CardDescription>
            {t("config.nsDescPre")}
            <code className="rounded bg-muted px-1">_global</code>
            {t("config.nsDescPost")}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex flex-wrap items-center gap-2">
            {nsNames.map((n) => {
              const dirty = namespaces?.find((x) => x.name === n)?.dirty;
              return (
                <button
                  key={n}
                  type="button"
                  aria-pressed={n === sel}
                  onClick={() => setSel(n)}
                  className={
                    "flex items-center gap-1 rounded-full px-3 py-1 text-sm transition-colors " +
                    (n === sel ? "bg-primary text-primary-foreground" : "bg-muted hover:bg-accent")
                  }
                >
                  {n}
                  {n === GLOBAL_NS && <span className="opacity-60">· {t("config.shared")}</span>}
                  {dirty && (
                    <span
                      className="size-1.5 rounded-full bg-amber-500"
                      title={t("config.unpublishedTitle")}
                      aria-label={t("config.unpublished")}
                    />
                  )}
                </button>
              );
            })}
            <Button size="sm" variant="outline" onClick={() => setNsOpen(true)}>
              {t("config.newNamespace")}
            </Button>
          </div>

          {/* Publish bar: version state + publish / history */}
          <div className="flex items-center gap-2">
            <Badge variant="secondary" className="font-mono">
              {nsView
                ? nsView.version
                  ? t("config.publishedV", { v: nsView.version })
                  : t("config.neverPublished")
                : "…"}
            </Badge>
            {nsView?.dirty && (
              <Badge variant="outline" className="border-amber-500/50 text-amber-600 dark:text-amber-400">
                {t("config.unpublished")}
              </Badge>
            )}
            <div className="flex-1" />
            <Button size="sm" variant="ghost" onClick={() => setHistOpen(true)}>
              {t("config.history")}
            </Button>
            <Button size="sm" variant="outline" onClick={() => setImportOpen(true)}>
              <FileUp /> {t("config.importEnv")}
            </Button>
            <Button size="sm" disabled={!nsView?.dirty} onClick={() => setPubOpen(true)}>
              {t("config.publish")}
            </Button>
          </div>

          {/* Unpublished changes (draft vs published), field-level */}
          {nsView?.dirty && diff?.changes.length ? (
            <div className="flex flex-col gap-1 rounded-lg border border-amber-500/30 bg-amber-500/5 p-3 text-sm">
              <div className="label-mono">
                {nsView.version
                  ? t("config.diffVsPublished", { v: nsView.version })
                  : t("config.draftNotPublished")}
              </div>
              {diff.changes.map((c) => (
                <div key={c.key} className="flex items-center gap-2">
                  <Badge variant="outline" className="w-20 justify-center text-xs">
                    {t(`config.status.${c.status}`)}
                  </Badge>
                  <code className="rounded bg-muted px-1">{c.key}</code>
                  {c.secret ? (
                    <span className="font-mono text-muted-foreground">••••••••</span>
                  ) : (
                    <span className="truncate font-mono text-muted-foreground">
                      {c.status !== "added" && <s>{JSON.stringify(c.old)}</s>}
                      {c.status === "modified" && " → "}
                      {c.status !== "removed" && JSON.stringify(c.new)}
                    </span>
                  )}
                </div>
              ))}
            </div>
          ) : null}

          {/* KV list for the selected namespace */}
          <div className="flex flex-col gap-2 rounded-lg border border-border/60 p-3">
            <div className="label-mono">
              {t("config.draftKeysPre")}
              <code className="rounded bg-muted px-1 normal-case">{sel}</code>
              {t("config.draftKeysPost")}
            </div>
            {nsView?.vars.length ? (
              nsView.vars.map((v) => (
                <div key={v.key} className="flex items-center gap-2 text-sm">
                  <code className="rounded bg-muted px-1">{v.key}</code>
                  <span className="flex-1 truncate font-mono text-muted-foreground">
                    {v.secret ? "••••••••" : v.value === undefined ? "—" : JSON.stringify(v.value)}
                  </span>
                  {v.is_json && <Badge variant="outline">{t("config.badge.json")}</Badge>}
                  {v.secret && <Badge variant="outline">{t("config.badge.secret")}</Badge>}
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={t("config.deleteKeyAria", { key: v.key })}
                    onClick={() => setDelKey(v.key)}
                  >
                    <Trash2 />
                  </Button>
                </div>
              ))
            ) : (
              <p className="text-sm text-muted-foreground">{t("config.noKeys")}</p>
            )}
            <div className="mt-1 flex items-end gap-2">
              <Field className="w-48">
                <FieldLabel htmlFor="nk">{t("config.key")}</FieldLabel>
                <Input id="nk" value={newKey} onChange={(e) => setNewKey(e.target.value)} placeholder="DB_URL" />
              </Field>
              <Field className="flex-1">
                <FieldLabel htmlFor="nv">{t("config.valueLabel")}</FieldLabel>
                <Input
                  id="nv"
                  value={newVal}
                  onChange={(e) => setNewVal(e.target.value)}
                  placeholder={t("config.valuePlaceholder")}
                />
              </Field>
              <Button
                variant="outline"
                disabled={!newKey.trim() || !!newKeyErr || savePatch.isPending}
                onClick={() => savePatch.mutate({ ns: sel, patch: { [newKey.trim()]: parseValue(newVal) } })}
              >
                {t("common.add")}
              </Button>
            </div>
            {newKeyErr && <p className="text-sm text-destructive">{newKeyErr}</p>}
          </div>

          <div className="rounded-lg bg-muted/50 p-3 text-xs text-muted-foreground">
            {t("config.consumeHint")}
            <pre className="mt-1 overflow-x-auto font-mono">
              <code>{`curl -H "X-Config-Token: shelf_…" ${resolveUrl}`}</code>
            </pre>
          </div>
        </CardContent>
      </Card>

      {/* Service tokens */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("config.serviceTokens")}</CardTitle>
          <CardDescription>
            {t("config.tokensDesc1")}
            <code className="rounded bg-muted px-1">X-Config-Token</code>
            {t("config.tokensDesc2")}
            <code className="rounded bg-muted px-1">_global</code>
            {t("config.tokensDesc3")}
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {clients?.length ? (
            clients.map((c) => (
              <div key={c.id} className="flex items-center gap-2 text-sm">
                <span className="font-medium">{c.name}</span>
                <span className="flex-1 truncate font-mono text-muted-foreground">
                  {c.namespaces.length ? c.namespaces.join(", ") : t("config.onlyGlobal")}
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("config.revokeTokenAria", { name: c.name })}
                  onClick={() => setDelClient(c.id)}
                >
                  <Trash2 />
                </Button>
              </div>
            ))
          ) : (
            <p className="text-sm text-muted-foreground">{t("config.noClients")}</p>
          )}
          <div>
            <Button variant="outline" onClick={() => setClientOpen(true)}>
              {t("config.newClient")}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* New namespace dialog */}
      <Dialog
        open={nsOpen}
        onOpenChange={(o) => {
          setNsOpen(o);
          if (!o) {
            setNsName("");
            setNsKey("");
            setNsVal("");
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("config.newNamespace")}</DialogTitle>
            <DialogDescription>{t("config.newNamespaceDesc")}</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <Field data-invalid={!!nsNameErr}>
              <FieldLabel htmlFor="nsn">{t("config.name")}</FieldLabel>
              <Input
                id="nsn"
                value={nsName}
                onChange={(e) => setNsName(e.target.value)}
                placeholder="service-a/prod"
                aria-invalid={!!nsNameErr}
              />
              {nsName && nsNameErr && <p className="text-sm text-destructive">{t(nsNameErr)}</p>}
            </Field>
            <div className="flex items-end gap-2">
              <Field className="w-40">
                <FieldLabel htmlFor="nsk">{t("config.firstKey")}</FieldLabel>
                <Input id="nsk" value={nsKey} onChange={(e) => setNsKey(e.target.value)} placeholder="DB_URL" />
              </Field>
              <Field className="flex-1">
                <FieldLabel htmlFor="nsv">{t("config.value")}</FieldLabel>
                <Input id="nsv" value={nsVal} onChange={(e) => setNsVal(e.target.value)} placeholder={t("config.valueOrJson")} />
              </Field>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setNsOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              disabled={!!nsNameErr || !nsKey.trim() || savePatch.isPending}
              onClick={() =>
                savePatch.mutate(
                  { ns: nsName.trim(), patch: { [nsKey.trim()]: parseValue(nsVal) } },
                  {
                    onSuccess: () => {
                      const ns = nsName.trim();
                      // Seed the list cache so the selection survives the guard
                      // effect before the invalidated refetch lands.
                      qc.setQueryData<NamespaceInfo[]>(["namespaces"], (old = []) =>
                        old.some((n) => n.name === ns)
                          ? old
                          : [...old, { name: ns, keys: 1, version: 0, dirty: true }],
                      );
                      setSel(ns);
                      setNsOpen(false);
                      setNsName("");
                      setNsKey("");
                      setNsVal("");
                    },
                  },
                )
              }
            >
              {t("common.create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* New client dialog */}
      <Dialog
        open={clientOpen}
        onOpenChange={(o) => {
          setClientOpen(o);
          if (!o) {
            setClientName("");
            setClientNs(new Set());
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("config.newToken")}</DialogTitle>
            <DialogDescription>{t("config.newTokenDesc")}</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <Field>
              <FieldLabel htmlFor="cn">{t("config.name")}</FieldLabel>
              <Input id="cn" value={clientName} onChange={(e) => setClientName(e.target.value)} placeholder="service-a" />
            </Field>
            <div>
              <div className="mb-1 text-sm font-medium">{t("config.namespaces")}</div>
              {serviceNs.length ? (
                <div className="flex flex-col gap-1">
                  {serviceNs.map((n) => (
                    <label key={n} className="flex items-center gap-2 text-sm">
                      <Checkbox
                        checked={clientNs.has(n)}
                        onCheckedChange={(c) =>
                          setClientNs((prev) => {
                            const next = new Set(prev);
                            if (c) next.add(n);
                            else next.delete(n);
                            return next;
                          })
                        }
                      />
                      {n}
                    </label>
                  ))}
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">
                  {t("config.noServiceNsPre")}
                  <code>_global</code>
                  {t("config.noServiceNsPost")}
                </p>
              )}
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setClientOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button disabled={!clientName.trim() || createClient.isPending} onClick={() => createClient.mutate()}>
              {t("common.create")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* One-time token reveal */}
      <Dialog open={!!issued} onOpenChange={(o) => !o && setIssued(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("config.tokenFor", { name: issued?.name ?? "" })}</DialogTitle>
            <DialogDescription>{t("config.tokenOnce")}</DialogDescription>
          </DialogHeader>
          <div className="flex items-center gap-2">
            <code className="flex-1 overflow-x-auto rounded bg-muted px-2 py-1 text-sm">{issued?.token}</code>
            <Button
              size="icon"
              variant="outline"
              aria-label={t("config.copyTokenAria")}
              onClick={async () => {
                if (!issued) return;
                try {
                  await navigator.clipboard.writeText(issued.token);
                  toast.success(t("common.copied"));
                } catch {
                  toast.error(t("config.toast.copyFailed"));
                }
              }}
            >
              <Copy />
            </Button>
          </div>
          <DialogFooter>
            <Button onClick={() => setIssued(null)}>{t("common.done")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ImportEnvDialog
        ns={sel}
        draft={nsView?.vars ?? []}
        open={importOpen}
        onOpenChange={setImportOpen}
        onImported={() => invalidateNs(sel)}
      />

      {/* Publish dialog */}
      <Dialog
        open={pubOpen}
        onOpenChange={(o) => {
          setPubOpen(o);
          if (!o) setPubNote("");
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("config.publishNs", { ns: sel })}</DialogTitle>
            <DialogDescription>
              {t("config.publishDesc", { v: (nsView?.version ?? 0) + 1 })}
            </DialogDescription>
          </DialogHeader>
          <Field>
            <FieldLabel htmlFor="pubnote">{t("config.noteOptional")}</FieldLabel>
            <Input
              id="pubnote"
              value={pubNote}
              onChange={(e) => setPubNote(e.target.value)}
              placeholder={t("config.notePlaceholder")}
            />
          </Field>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPubOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button disabled={publish.isPending} onClick={() => publish.mutate()}>
              {t("config.publish")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Version history */}
      <Dialog
        open={histOpen}
        onOpenChange={(o) => {
          setHistOpen(o);
          if (!o) setViewVer(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("config.historyTitle", { ns: sel })}</DialogTitle>
            <DialogDescription>{t("config.historyDesc")}</DialogDescription>
          </DialogHeader>
          <div className="flex max-h-80 flex-col gap-2 overflow-y-auto">
            {versions?.length ? (
              versions.map((v) => (
                <div key={v.version} className="rounded-lg border border-border/60 p-2 text-sm">
                  <div className="flex items-center gap-2">
                    <Badge variant={v.version === nsView?.version ? "default" : "secondary"} className="font-mono">
                      v{v.version}
                    </Badge>
                    <span className="flex-1 truncate">
                      {v.note || <span className="text-muted-foreground">{t("config.noNote")}</span>}
                    </span>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => setViewVer(viewVer === v.version ? null : v.version)}
                    >
                      {viewVer === v.version ? t("config.hide") : t("config.view")}
                    </Button>
                    <Button size="sm" variant="outline" disabled={rollback.isPending} onClick={() => rollback.mutate(v.version)}>
                      {t("config.rollback")}
                    </Button>
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {t("config.keysCount", { n: v.keys })} · {v.author || "—"} · {fmtTime(v.published_at)}
                  </div>
                  {viewVer === v.version && (
                    <div className="mt-2 flex flex-col gap-1 border-t border-border/60 pt-2">
                      {verVars === undefined ? (
                        <span className="text-xs text-muted-foreground">{t("common.loading")}</span>
                      ) : verVars.length ? (
                        verVars.map((k) => (
                          <div key={k.key} className="flex items-center gap-2">
                            <code className="rounded bg-muted px-1">{k.key}</code>
                            <span className="flex-1 truncate font-mono text-muted-foreground">
                              {k.secret ? "••••••••" : k.value === undefined ? "—" : JSON.stringify(k.value)}
                            </span>
                          </div>
                        ))
                      ) : (
                        <span className="text-xs text-muted-foreground">{t("config.empty")}</span>
                      )}
                    </div>
                  )}
                </div>
              ))
            ) : (
              <p className="text-sm text-muted-foreground">{t("config.noVersions")}</p>
            )}
          </div>
        </DialogContent>
      </Dialog>

      {/* Delete key confirm */}
      <AlertDialog open={!!delKey} onOpenChange={(o) => !o && setDelKey(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("config.deleteKeyTitle", { key: delKey ?? "" })}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("config.deleteKeyDescPre")}
              <code>{sel}</code>
              {t("config.deleteKeyDescPost")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (delKey) savePatch.mutate({ ns: sel, patch: { [delKey]: null } });
                setDelKey(null);
              }}
            >
              {t("common.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Revoke confirm */}
      <AlertDialog open={!!delClient} onOpenChange={(o) => !o && setDelClient(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("config.revokeTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("config.revokeDesc")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => delClient && removeClient.mutate(delClient)}>
              {t("config.revoke")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
