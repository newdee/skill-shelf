import { useState } from "react";
import { getApiBase, setApiBase } from "../lib/config";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { useT } from "../lib/i18n";
import { ConfigPanel } from "../components/ConfigPanel";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";

export function Settings() {
  const { canWrite } = useAuth();
  const { t } = useT();
  const [url, setUrl] = useState(getApiBase());
  const [result, setResult] = useState<string | null>(null);
  const urlErr = !url.trim()
    ? t("settings.urlRequired")
    : !/^https?:\/\/.+/i.test(url.trim())
      ? t("settings.urlScheme")
      : null;

  function save() {
    setApiBase(url);
    setUrl(getApiBase());
    setResult(t("settings.saved"));
  }
  async function test() {
    setResult(t("settings.testing"));
    try {
      setApiBase(url);
      const s = await api.status();
      setResult(
        t("settings.ok", { service: s.service, version: s.version }) +
          (s.auth_enabled ? t("settings.authOn") : ""),
      );
    } catch (e) {
      setResult(t("settings.failed", { msg: (e as Error).message }));
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <h1>{t("settings.title")}</h1>

      {canWrite && <ConfigPanel />}

      <Card>
        <CardHeader>
          <CardTitle className="text-base">{t("settings.backend")}</CardTitle>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field data-invalid={!!urlErr}>
              <FieldLabel htmlFor="url">{t("settings.backendAddress")}</FieldLabel>
              <Input
                id="url"
                className="font-mono"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder={t("settings.backendPh")}
                aria-invalid={!!urlErr}
              />
              {urlErr ? (
                <FieldError>{urlErr}</FieldError>
              ) : (
                <FieldDescription>{t("settings.backendHint")}</FieldDescription>
              )}
            </Field>
            <div className="flex gap-2">
              <Button disabled={!!urlErr} onClick={save}>
                {t("settings.save")}
              </Button>
              <Button variant="outline" disabled={!!urlErr} onClick={test}>
                {t("settings.test")}
              </Button>
            </div>
            {result && <p className="text-sm text-muted-foreground">{result}</p>}
          </FieldGroup>
        </CardContent>
      </Card>
    </div>
  );
}
