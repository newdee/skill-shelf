import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { api, type ConfigView } from "../lib/api";
import { useT } from "../lib/i18n";
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
  const { t } = useT();
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
      toast.success(t("configpanel.updated"));
    },
    onError: (e) => toast.error(t("configpanel.updateFailed", { msg: (e as Error).message })),
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
          {t("configpanel.title")}
          <Badge variant={cfg?.ai_ready ? "secondary" : "outline"}>
            {cfg?.ai_ready ? t("configpanel.aiReady") : t("configpanel.aiOff")}
          </Badge>
        </CardTitle>
        <CardDescription>{t("configpanel.desc")}</CardDescription>
      </CardHeader>
      <CardContent>
        <FieldGroup>
          <FieldLabel className="label-mono">{t("configpanel.aiSection")}</FieldLabel>
          <div className="flex gap-3">
            <Field className="flex-1">
              <FieldLabel htmlFor="aib">{t("configpanel.baseUrl")}</FieldLabel>
              <Input
                id="aib"
                className="font-mono"
                value={aiBase}
                onChange={(e) => setAiBase(e.target.value)}
                placeholder={t("configpanel.baseUrlPh")}
              />
            </Field>
            <Field className="w-48">
              <FieldLabel htmlFor="aim">{t("configpanel.model")}</FieldLabel>
              <Input
                id="aim"
                className="font-mono"
                value={aiModel}
                onChange={(e) => setAiModel(e.target.value)}
                placeholder={t("configpanel.modelPh")}
              />
            </Field>
          </div>
          <Field>
            <FieldLabel htmlFor="aik">{t("configpanel.apiKey")}</FieldLabel>
            <Input
              id="aik"
              type="password"
              value={aiKey}
              onChange={(e) => setAiKey(e.target.value)}
              placeholder={isSet(cfg, "AI_API_KEY") ? t("configpanel.secretSetKeep") : t("configpanel.apiKeyPh")}
            />
          </Field>

          <FieldLabel className="label-mono mt-2">{t("configpanel.githubSection")}</FieldLabel>
          <div className="flex gap-3">
            <Field className="flex-1">
              <FieldLabel htmlFor="ghb">{t("configpanel.apiBase")}</FieldLabel>
              <Input
                id="ghb"
                className="font-mono"
                value={ghBase}
                onChange={(e) => setGhBase(e.target.value)}
                placeholder={t("configpanel.apiBasePh")}
              />
            </Field>
            <Field className="flex-1">
              <FieldLabel htmlFor="ght">{t("configpanel.token")}</FieldLabel>
              <Input
                id="ght"
                type="password"
                value={ghToken}
                onChange={(e) => setGhToken(e.target.value)}
                placeholder={isSet(cfg, "GITHUB_TOKEN") ? t("configpanel.secretSet") : t("configpanel.tokenPh")}
              />
            </Field>
          </div>

          <div>
            <Button disabled={save.isPending} onClick={saveKnown}>
              {t("configpanel.save")}
            </Button>
          </div>

          <FieldLabel className="label-mono mt-2">{t("configpanel.customVars")}</FieldLabel>
          {custom.map((v) => (
            <div key={v.key} className="flex items-center gap-2 text-sm">
              <code className="rounded bg-muted px-1">{v.key}</code>
              <span className="flex-1 truncate font-mono text-muted-foreground">
                {v.secret ? "••••••••" : JSON.stringify(v.value)}
              </span>
              {v.is_json && (
                <Badge variant="outline" className="font-mono">
                  json
                </Badge>
              )}
              <Button
                variant="ghost"
                size="icon"
                aria-label={t("configpanel.remove")}
                onClick={() => save.mutate({ [v.key]: null })}
              >
                <Trash2 />
              </Button>
            </div>
          ))}
          <div className="flex items-end gap-2">
            <Field className="w-48">
              <FieldLabel htmlFor="nk">{t("configpanel.key")}</FieldLabel>
              <Input
                id="nk"
                className="font-mono"
                value={newKey}
                onChange={(e) => setNewKey(e.target.value)}
                placeholder={t("configpanel.keyPh")}
              />
            </Field>
            <Field className="flex-1">
              <FieldLabel htmlFor="nv">{t("configpanel.value")}</FieldLabel>
              <Input
                id="nv"
                className="font-mono"
                value={newVal}
                onChange={(e) => setNewVal(e.target.value)}
                placeholder={t("configpanel.valuePh")}
              />
            </Field>
            <Button
              variant="outline"
              disabled={!newKey.trim() || save.isPending}
              onClick={() => save.mutate({ [newKey.trim()]: parseValue(newVal) })}
            >
              {t("configpanel.add")}
            </Button>
          </div>
          <FieldDescription>{t("configpanel.hint")}</FieldDescription>
        </FieldGroup>
      </CardContent>
    </Card>
  );
}
