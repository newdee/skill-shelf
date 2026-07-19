import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Download, Upload, Trash2, GitBranch, Plus, Search, Lock } from "lucide-react";
import { toast } from "sonner";
import { api, type Kind } from "../lib/api";
import { useAuth } from "../lib/auth";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Skeleton } from "@/components/ui/skeleton";
import { Alert, AlertTitle } from "@/components/ui/alert";
import { Empty, EmptyDescription, EmptyTitle } from "@/components/ui/empty";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
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

// Mirror the backend's spec rules so authors get instant feedback.
function nameError(name: string, kind: Kind): string | null {
  const n = name.trim();
  if (!n) return "Name is required";
  if (kind === "skill") {
    if (n.length > 64) return "Max 64 characters";
    if (!/^[a-z0-9-]+$/.test(n)) return "Lowercase letters, digits, and hyphens only";
    if (n.startsWith("-") || n.endsWith("-")) return "No leading or trailing hyphen";
    if (n.includes("--")) return "No consecutive hyphens";
  }
  return null;
}

export function SkillList() {
  const qc = useQueryClient();
  const { canWrite, authEnabled } = useAuth();

  const [q, setQ] = useState("");
  const { data: skills, isLoading, error } = useQuery({
    queryKey: ["skills", q],
    queryFn: () => api.listSkills(q.trim() ? { q: q.trim() } : undefined),
  });

  const [sel, setSel] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState<string[] | null>(null);
  const [newOpen, setNewOpen] = useState(false);

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [kind, setKind] = useState<Kind>("skill");
  const [ghUrl, setGhUrl] = useState("");
  const fileInput = useRef<HTMLInputElement>(null);

  const invalidate = () => qc.invalidateQueries({ queryKey: ["skills"] });
  const nErr = nameError(name, kind);
  const dErr = description.trim() ? null : "Description is required";

  const create = useMutation({
    mutationFn: () => api.createSkill({ name: name.trim(), description: description.trim(), kind }),
    onSuccess: () => {
      setName("");
      setDescription("");
      setNewOpen(false);
      invalidate();
      toast.success("Created");
    },
    onError: (e) => toast.error(`Create failed: ${(e as Error).message}`),
  });

  const importZip = useMutation({
    mutationFn: (file: File) => api.importZip(file),
    onSuccess: (s) => {
      setNewOpen(false);
      invalidate();
      toast.success(`Imported ${s.name}`);
    },
    onError: (e) => toast.error(`Import failed: ${(e as Error).message}`),
  });

  const importGithub = useMutation({
    mutationFn: () => api.importGithub({ url: ghUrl.trim() }),
    onSuccess: (r) => {
      setGhUrl("");
      setNewOpen(false);
      invalidate();
      toast.success(`Imported ${r.imported.length}: ${r.imported.map((s) => s.name).join(", ")}`);
      if (r.skipped.length) toast.warning(`Skipped ${r.skipped.length}`);
    },
    onError: (e) => toast.error(`GitHub import failed: ${(e as Error).message}`),
  });

  const del = useMutation({
    mutationFn: (ids: string[]) => Promise.all(ids.map((id) => api.deleteSkill(id))),
    onSuccess: (_r, ids) => {
      setSel(new Set());
      setConfirm(null);
      invalidate();
      toast.success(`Deleted ${ids.length}`);
    },
    onError: (e) => {
      setConfirm(null);
      toast.error(`Delete failed: ${(e as Error).message}`);
    },
  });

  function toggle(id: string) {
    setSel((s) => {
      const n = new Set(s);
      n.has(id) ? n.delete(id) : n.add(id);
      return n;
    });
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <h1 className="text-2xl font-semibold tracking-tight">Skills</h1>
        <div className="flex-1" />
        {canWrite && sel.size > 0 && (
          <Button variant="outline" onClick={() => setConfirm([...sel])}>
            <Trash2 data-icon="inline-start" />
            Delete ({sel.size})
          </Button>
        )}
        {canWrite && (
          <Button onClick={() => setNewOpen(true)}>
            <Plus data-icon="inline-start" />
            New
          </Button>
        )}
      </div>

      <div className="relative">
        <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          className="pl-9"
          placeholder="Search skills and prompts…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </div>

      {!canWrite && (
        <p className="flex items-center gap-1.5 text-sm text-muted-foreground">
          <Lock className="size-3.5" />
          Read-only — {authEnabled ? "sign in as an admin (top-right menu) to create or delete." : "open instance."}
        </p>
      )}

      {isLoading && (
        <div className="grid gap-3 sm:grid-cols-2">
          <Skeleton className="h-28" />
          <Skeleton className="h-28" />
        </div>
      )}
      {error && (
        <Alert variant="destructive">
          <AlertTitle>Cannot reach backend: {(error as Error).message}</AlertTitle>
        </Alert>
      )}

      <div className="grid gap-3 sm:grid-cols-2">
        {skills?.map((s) => (
          <Card key={s.id} className="gap-0">
            <CardHeader>
              <CardTitle className="flex items-start gap-2 text-base">
                {canWrite && (
                  <Checkbox
                    className="mt-0.5"
                    checked={sel.has(s.id)}
                    onCheckedChange={() => toggle(s.id)}
                    aria-label={`Select ${s.name}`}
                  />
                )}
                <Link to="/skill/$id" params={{ id: s.id }} className="hover:underline">
                  {s.name}
                </Link>
                <Badge variant={s.kind === "prompt" ? "outline" : "secondary"}>{s.kind}</Badge>
                {s.license && <Badge variant="outline">{s.license}</Badge>}
              </CardTitle>
            </CardHeader>
            <CardContent className="flex flex-col gap-3 pt-3">
              <p className="line-clamp-2 min-h-[2.5rem] text-sm text-muted-foreground">
                {s.description || "No description."}
              </p>
              <div className="flex items-center gap-1">
                <Button variant="ghost" size="sm" asChild>
                  <a href={api.exportUrl(s.id)}>
                    <Download data-icon="inline-start" />
                    Export
                  </a>
                </Button>
                {canWrite && (
                  <Button variant="ghost" size="sm" onClick={() => setConfirm([s.id])}>
                    <Trash2 data-icon="inline-start" />
                    Delete
                  </Button>
                )}
              </div>
            </CardContent>
          </Card>
        ))}
      </div>

      {skills?.length === 0 && (
        <Empty>
          <EmptyTitle>{q ? "No matches" : "No skills yet"}</EmptyTitle>
          <EmptyDescription>
            {q ? "Try a different search." : canWrite ? "Create one or import from a .zip / GitHub." : "Nothing here yet."}
          </EmptyDescription>
        </Empty>
      )}

      {/* New skill dialog */}
      <Dialog open={newOpen} onOpenChange={setNewOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>New</DialogTitle>
          </DialogHeader>
          <FieldGroup>
            <div className="flex gap-3">
              <Field className="flex-1" data-invalid={!!nErr && !!name}>
                <FieldLabel htmlFor="name">Name *</FieldLabel>
                <Input
                  id="name"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="pdf-parse"
                  aria-invalid={!!nErr && !!name}
                />
                {!!name && nErr && <FieldError>{nErr}</FieldError>}
              </Field>
              <Field className="w-36">
                <FieldLabel htmlFor="kind">Kind</FieldLabel>
                <Select value={kind} onValueChange={(v) => setKind(v as Kind)}>
                  <SelectTrigger id="kind">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      <SelectItem value="skill">skill</SelectItem>
                      <SelectItem value="prompt">prompt</SelectItem>
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
            </div>
            <Field data-invalid={!!dErr && !!description}>
              <FieldLabel htmlFor="desc">Description *</FieldLabel>
              <Input
                id="desc"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="what it does + when to use (routing signal)"
                aria-invalid={!!dErr && !!description}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="gh">Or import from GitHub</FieldLabel>
              <div className="flex gap-2">
                <Input
                  id="gh"
                  className="flex-1"
                  value={ghUrl}
                  onChange={(e) => setGhUrl(e.target.value)}
                  placeholder="github.com/owner/repo"
                />
                <Button
                  variant="outline"
                  disabled={!ghUrl.trim() || importGithub.isPending}
                  onClick={() => importGithub.mutate()}
                >
                  <GitBranch data-icon="inline-start" />
                  Pull
                </Button>
              </div>
            </Field>
          </FieldGroup>
          <DialogFooter>
            <Button variant="outline" onClick={() => fileInput.current?.click()} disabled={importZip.isPending}>
              <Upload data-icon="inline-start" />
              Import .zip
            </Button>
            <Button disabled={!!nErr || !!dErr || create.isPending} onClick={() => create.mutate()}>
              Create
            </Button>
          </DialogFooter>
          <input
            ref={fileInput}
            type="file"
            accept=".zip"
            className="hidden"
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) importZip.mutate(f);
              e.target.value = "";
            }}
          />
        </DialogContent>
      </Dialog>

      {/* Delete confirmation */}
      <AlertDialog open={!!confirm} onOpenChange={(o) => !o && setConfirm(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {confirm?.length} item{confirm && confirm.length > 1 ? "s" : ""}?</AlertDialogTitle>
            <AlertDialogDescription>
              This permanently removes the {confirm && confirm.length > 1 ? "skills" : "skill"} and all{" "}
              {confirm && confirm.length > 1 ? "their" : "its"} versions. This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => confirm && del.mutate(confirm)}
              className="bg-destructive text-white hover:bg-destructive/90"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
