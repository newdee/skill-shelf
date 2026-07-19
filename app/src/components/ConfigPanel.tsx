import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { api, type ConfigView } from "../lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Trash2 } from "lucide-react";

const KNOWN = ["AI_BASE_URL", "AI_MODEL", "AI_API_KEY", "GITHUB_API_BASE", "GITHUB_TOKEN"];

function val(v: ConfigView | undefined, key: string): string {
  const e = v?.vars.find((x) => x.key === key);
  return e && typeof e.value === "string" ? e.value : "";
}
function isSet(v: ConfigView | undefined, key: string): boolean {
  return !!v?.vars.find((x) => x.key === key)?.set;
}

/** Parse an input as JSON if it's valid JSON, otherwise treat it as a string. */
function parseValue(raw: string): unknown {
  try {
    const p = JSON.parse(raw);
    return typeof p === "object" || typeof p === "number" || typeof p === "boolean" ? p : raw;
  } catch {
    return raw;
  }
}

export function ConfigPanel() {
  const qc = useQueryClient();
  const { data: cfg } = useQuery({ queryKey: ["config"], queryFn: api.getConfig });

  const [aiBase, setAiBase] = useState("");
  const [aiModel, setAiModel] = useState("");
  const [aiKey, setAiKey] = useState("");
  const [ghBase, setGhBase] = useState("");
  const [ghToken, setGhToken] = useState("");
  const [newKey, setNewKey] = useState("");
  const [newVal, setNewVal] = useState("");

  useEffect(() => {
    if (!cfg) return;
    setAiBase(val(cfg, "AI_BASE_URL"));
    setAiModel(val(cfg, "AI_MODEL"));
    setGhBase(val(cfg, "GITHUB_API_BASE"));
  }, [cfg]);

  const save = useMutation({
    mutationFn: (patch: Record<string, unknown>) => api.putConfig(patch),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["config"] });
      setAiKey("");
      setGhToken("");
      setNewKey("");
      setNewVal("");
      toast.success("Config updated");
    },
    onError: (e) => toast.error(`Update failed: ${(e as Error).message}`),
  });

  function saveKnown() {
    const patch: Record<string, unknown> = {
      AI_BASE_URL: aiBase,
      AI_MODEL: aiModel,
      GITHUB_API_BASE: ghBase,
    };
    if (aiKey.trim()) patch.AI_API_KEY = aiKey.trim();
    if (ghToken.trim()) patch.GITHUB_TOKEN = ghToken.trim();
    save.mutate(patch);
  }

  const custom = cfg?.vars.filter((v) => !KNOWN.includes(v.key)) ?? [];

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          Service settings
          <Badge variant={cfg?.ai_ready ? "secondary" : "outline"}>
            AI {cfg?.ai_ready ? "ready" : "off"}
          </Badge>
        </CardTitle>
        <CardDescription>
          Settings this Skill Shelf instance uses itself (AI, GitHub). Config for other services lives in
          the Config center.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <FieldLabel className="text-xs text-muted-foreground">AI (OpenAI-compatible)</FieldLabel>
          <div className="flex gap-3">
            <Field className="flex-1">
              <FieldLabel htmlFor="aib">Base URL</FieldLabel>
              <Input id="aib" value={aiBase} onChange={(e) => setAiBase(e.target.value)} placeholder="https://api.openai.com/v1" />
            </Field>
            <Field className="w-48">
              <FieldLabel htmlFor="aim">Model</FieldLabel>
              <Input id="aim" value={aiModel} onChange={(e) => setAiModel(e.target.value)} placeholder="gpt-4o-mini" />
            </Field>
          </div>
          <Field>
            <FieldLabel htmlFor="aik">API key</FieldLabel>
            <Input
              id="aik"
              type="password"
              value={aiKey}
              onChange={(e) => setAiKey(e.target.value)}
              placeholder={isSet(cfg, "AI_API_KEY") ? "•••••••• (set — leave blank to keep)" : "sk-…"}
            />
          </Field>

          <FieldLabel className="mt-2 text-xs text-muted-foreground">GitHub</FieldLabel>
          <div className="flex gap-3">
            <Field className="flex-1">
              <FieldLabel htmlFor="ghb">API base</FieldLabel>
              <Input id="ghb" value={ghBase} onChange={(e) => setGhBase(e.target.value)} placeholder="https://api.github.com" />
            </Field>
            <Field className="flex-1">
              <FieldLabel htmlFor="ght">Token</FieldLabel>
              <Input
                id="ght"
                type="password"
                value={ghToken}
                onChange={(e) => setGhToken(e.target.value)}
                placeholder={isSet(cfg, "GITHUB_TOKEN") ? "•••••••• (set)" : "ghp_…"}
              />
            </Field>
          </div>

          <div>
            <Button disabled={save.isPending} onClick={saveKnown}>
              Save
            </Button>
          </div>

          <FieldLabel className="mt-2 text-xs text-muted-foreground">Custom variables (string or JSON)</FieldLabel>
          {custom.map((v) => (
            <div key={v.key} className="flex items-center gap-2 text-sm">
              <code className="rounded bg-muted px-1">{v.key}</code>
              <span className="flex-1 truncate text-muted-foreground">
                {v.secret ? "••••••••" : JSON.stringify(v.value)}
              </span>
              {v.is_json && <Badge variant="outline">json</Badge>}
              <Button variant="ghost" size="icon" onClick={() => save.mutate({ [v.key]: null })}>
                <Trash2 />
              </Button>
            </div>
          ))}
          <div className="flex items-end gap-2">
            <Field className="w-48">
              <FieldLabel htmlFor="nk">Key</FieldLabel>
              <Input id="nk" value={newKey} onChange={(e) => setNewKey(e.target.value)} placeholder="MY_VAR" />
            </Field>
            <Field className="flex-1">
              <FieldLabel htmlFor="nv">Value (string or JSON)</FieldLabel>
              <Input id="nv" value={newVal} onChange={(e) => setNewVal(e.target.value)} placeholder='hello  or  {"a":1}' />
            </Field>
            <Button
              variant="outline"
              disabled={!newKey.trim() || save.isPending}
              onClick={() => save.mutate({ [newKey.trim()]: parseValue(newVal) })}
            >
              Add
            </Button>
          </div>
          <FieldDescription>
            Applied immediately, no restart. Structural settings (PORT, DATA_DIR, DB, JWT_SECRET) are startup-only.
          </FieldDescription>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
