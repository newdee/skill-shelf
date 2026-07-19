import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { api, type NamespaceInfo, type NewClient } from "../lib/api";
import { useAuth } from "../lib/auth";
import { getApiBase } from "../lib/config";
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
import { Copy, Trash2 } from "lucide-react";

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

/** Same rule as the backend `valid_namespace`. */
function nsError(name: string): string | null {
  const n = name.trim();
  if (!n) return "Namespace is required";
  if (n.length > 128) return "Max 128 characters";
  if (n.includes("..")) return "Must not contain '..'";
  if (!/^[a-zA-Z0-9._/-]+$/.test(n)) return "Letters, digits, and - _ / . only";
  return null;
}

export function ConfigCenter() {
  const qc = useQueryClient();
  const { canWrite } = useAuth();
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
  const [delKey, setDelKey] = useState<string | null>(null);

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
      qc.invalidateQueries({ queryKey: ["namespace", v.ns] });
      qc.invalidateQueries({ queryKey: ["namespaces"] });
      setNewKey("");
      setNewVal("");
    },
    onError: (e) => toast.error(`Save failed: ${(e as Error).message}`),
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
    onError: (e) => toast.error(`Create failed: ${(e as Error).message}`),
  });

  const removeClient = useMutation({
    mutationFn: (id: string) => api.deleteClient(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["clients"] });
      setDelClient(null);
      toast.success("Client revoked");
    },
    onError: (e) => toast.error(`Revoke failed: ${(e as Error).message}`),
  });

  // keep the selected namespace valid if the list changes — but only once the
  // list has actually loaded, so a freshly-created (optimistically-seeded)
  // namespace isn't reset out from under the user mid-refetch.
  useEffect(() => {
    if (namespaces && !nsNames.includes(sel)) setSel(GLOBAL_NS);
  }, [namespaces, nsNames, sel]);

  const newKeyErr = newKey.trim() && nsView?.vars.some((v) => v.key === newKey.trim())
    ? "Key already exists"
    : null;
  const nsNameErr = nsError(nsName);
  const resolveUrl = `${getApiBase()}/config/resolve?namespace=${sel}`;

  if (!canWrite) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Admin only</CardTitle>
          <CardDescription>
            The config center is available to administrators. Sign in from the top-right menu.
          </CardDescription>
        </CardHeader>
      </Card>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Config center</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Other services fetch their config from here instead of reading env vars.
        </p>
      </div>

      {(nsQuery.isError || clientsError) && (
        <div className="rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
          Couldn't load config. Check you're signed in as an admin and the backend is reachable.
        </div>
      )}

      {/* Namespaces + KV editor */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Namespaces</CardTitle>
          <CardDescription>
            Each service/environment is a namespace. <code className="rounded bg-muted px-1">_global</code> is
            merged into every fetch — keep only shared, non-secret defaults there.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="flex flex-wrap items-center gap-2">
            {nsNames.map((n) => (
              <button
                key={n}
                type="button"
                aria-pressed={n === sel}
                onClick={() => setSel(n)}
                className={
                  "rounded-full px-3 py-1 text-sm transition-colors " +
                  (n === sel ? "bg-primary text-primary-foreground" : "bg-muted hover:bg-accent")
                }
              >
                {n}
                {n === GLOBAL_NS && <span className="ml-1 opacity-60">· shared</span>}
              </button>
            ))}
            <Button size="sm" variant="outline" onClick={() => setNsOpen(true)}>
              New namespace
            </Button>
          </div>

          {/* KV list for the selected namespace */}
          <div className="flex flex-col gap-2 rounded-lg border border-border/60 p-3">
            <div className="text-xs font-medium text-muted-foreground">
              Keys in <code className="rounded bg-muted px-1">{sel}</code>
            </div>
            {nsView?.vars.length ? (
              nsView.vars.map((v) => (
                <div key={v.key} className="flex items-center gap-2 text-sm">
                  <code className="rounded bg-muted px-1">{v.key}</code>
                  <span className="flex-1 truncate text-muted-foreground">
                    {v.secret ? "••••••••" : v.value === undefined ? "—" : JSON.stringify(v.value)}
                  </span>
                  {v.is_json && <Badge variant="outline">json</Badge>}
                  {v.secret && <Badge variant="outline">secret</Badge>}
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={`Delete key ${v.key}`}
                    onClick={() => setDelKey(v.key)}
                  >
                    <Trash2 />
                  </Button>
                </div>
              ))
            ) : (
              <p className="text-sm text-muted-foreground">No keys yet.</p>
            )}
            <div className="mt-1 flex items-end gap-2">
              <Field className="w-48">
                <FieldLabel htmlFor="nk">Key</FieldLabel>
                <Input id="nk" value={newKey} onChange={(e) => setNewKey(e.target.value)} placeholder="DB_URL" />
              </Field>
              <Field className="flex-1">
                <FieldLabel htmlFor="nv">Value (string or JSON)</FieldLabel>
                <Input
                  id="nv"
                  value={newVal}
                  onChange={(e) => setNewVal(e.target.value)}
                  placeholder='postgres://…  or  {"max":5}'
                />
              </Field>
              <Button
                variant="outline"
                disabled={!newKey.trim() || !!newKeyErr || savePatch.isPending}
                onClick={() => savePatch.mutate({ ns: sel, patch: { [newKey.trim()]: parseValue(newVal) } })}
              >
                Add
              </Button>
            </div>
            {newKeyErr && <p className="text-sm text-destructive">{newKeyErr}</p>}
          </div>

          <div className="rounded-lg bg-muted/50 p-3 text-xs text-muted-foreground">
            Consume with a service token:
            <pre className="mt-1 overflow-x-auto">
              <code>{`curl -H "X-Config-Token: shelf_…" ${resolveUrl}`}</code>
            </pre>
          </div>
        </CardContent>
      </Card>

      {/* Service tokens */}
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Service tokens</CardTitle>
          <CardDescription>
            Each client gets a token (sent as <code className="rounded bg-muted px-1">X-Config-Token</code>) that
            may read its granted namespaces. <code className="rounded bg-muted px-1">_global</code> is always
            included.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {clients?.length ? (
            clients.map((c) => (
              <div key={c.id} className="flex items-center gap-2 text-sm">
                <span className="font-medium">{c.name}</span>
                <span className="flex-1 truncate text-muted-foreground">
                  {c.namespaces.length ? c.namespaces.join(", ") : "(only _global)"}
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={`Revoke token ${c.name}`}
                  onClick={() => setDelClient(c.id)}
                >
                  <Trash2 />
                </Button>
              </div>
            ))
          ) : (
            <p className="text-sm text-muted-foreground">No clients yet.</p>
          )}
          <div>
            <Button variant="outline" onClick={() => setClientOpen(true)}>
              New client
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
            <DialogTitle>New namespace</DialogTitle>
            <DialogDescription>Create a namespace and its first key.</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <Field data-invalid={!!nsNameErr}>
              <FieldLabel htmlFor="nsn">Name</FieldLabel>
              <Input
                id="nsn"
                value={nsName}
                onChange={(e) => setNsName(e.target.value)}
                placeholder="service-a/prod"
                aria-invalid={!!nsNameErr}
              />
              {nsName && nsNameErr && <p className="text-sm text-destructive">{nsNameErr}</p>}
            </Field>
            <div className="flex items-end gap-2">
              <Field className="w-40">
                <FieldLabel htmlFor="nsk">First key</FieldLabel>
                <Input id="nsk" value={nsKey} onChange={(e) => setNsKey(e.target.value)} placeholder="DB_URL" />
              </Field>
              <Field className="flex-1">
                <FieldLabel htmlFor="nsv">Value</FieldLabel>
                <Input id="nsv" value={nsVal} onChange={(e) => setNsVal(e.target.value)} placeholder="value or JSON" />
              </Field>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setNsOpen(false)}>
              Cancel
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
                        old.some((n) => n.name === ns) ? old : [...old, { name: ns, keys: 1 }],
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
              Create
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
            <DialogTitle>New service token</DialogTitle>
            <DialogDescription>Grant the namespaces this service may read.</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <Field>
              <FieldLabel htmlFor="cn">Name</FieldLabel>
              <Input id="cn" value={clientName} onChange={(e) => setClientName(e.target.value)} placeholder="service-a" />
            </Field>
            <div>
              <div className="mb-1 text-sm font-medium">Namespaces</div>
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
                  No service namespaces yet — this token will read only <code>_global</code>.
                </p>
              )}
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setClientOpen(false)}>
              Cancel
            </Button>
            <Button disabled={!clientName.trim() || createClient.isPending} onClick={() => createClient.mutate()}>
              Create
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* One-time token reveal */}
      <Dialog open={!!issued} onOpenChange={(o) => !o && setIssued(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Token for {issued?.name}</DialogTitle>
            <DialogDescription>
              Copy it now — it is shown only once and cannot be recovered.
            </DialogDescription>
          </DialogHeader>
          <div className="flex items-center gap-2">
            <code className="flex-1 overflow-x-auto rounded bg-muted px-2 py-1 text-sm">{issued?.token}</code>
            <Button
              size="icon"
              variant="outline"
              aria-label="Copy token"
              onClick={async () => {
                if (!issued) return;
                try {
                  await navigator.clipboard.writeText(issued.token);
                  toast.success("Copied");
                } catch {
                  toast.error("Couldn't copy — select the token and copy it manually");
                }
              }}
            >
              <Copy />
            </Button>
          </div>
          <DialogFooter>
            <Button onClick={() => setIssued(null)}>Done</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete key confirm */}
      <AlertDialog open={!!delKey} onOpenChange={(o) => !o && setDelKey(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete key {delKey}?</AlertDialogTitle>
            <AlertDialogDescription>
              Services resolving <code>{sel}</code> will stop receiving this key. This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (delKey) savePatch.mutate({ ns: sel, patch: { [delKey]: null } });
                setDelKey(null);
              }}
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Revoke confirm */}
      <AlertDialog open={!!delClient} onOpenChange={(o) => !o && setDelClient(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Revoke this token?</AlertDialogTitle>
            <AlertDialogDescription>
              The service using it will immediately lose access. This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => delClient && removeClient.mutate(delClient)}>
              Revoke
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
