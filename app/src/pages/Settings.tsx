import { useState } from "react";
import { getApiBase, setApiBase } from "../lib/config";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth";
import { ConfigPanel } from "../components/ConfigPanel";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";

export function Settings() {
  const { canWrite } = useAuth();
  const [url, setUrl] = useState(getApiBase());
  const [result, setResult] = useState<string | null>(null);
  const urlErr = !url.trim()
    ? "Backend address is required"
    : !/^https?:\/\/.+/i.test(url.trim())
      ? "Must start with http:// or https://"
      : null;

  function save() {
    setApiBase(url);
    setUrl(getApiBase());
    setResult("Saved.");
  }
  async function test() {
    setResult("Testing…");
    try {
      setApiBase(url);
      const s = await api.status();
      setResult(`OK — ${s.service} v${s.version}${s.auth_enabled ? " (auth on)" : ""}`);
    } catch (e) {
      setResult(`Failed: ${(e as Error).message}`);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>

      {canWrite && <ConfigPanel />}

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Backend</CardTitle>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field data-invalid={!!urlErr}>
              <FieldLabel htmlFor="url">Backend address</FieldLabel>
              <Input
                id="url"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="http://127.0.0.1:8080"
                aria-invalid={!!urlErr}
              />
              {urlErr ? (
                <FieldError>{urlErr}</FieldError>
              ) : (
                <FieldDescription>
                  Applied at runtime — the same build can point at any server. Sign in from the top-right menu.
                </FieldDescription>
              )}
            </Field>
            <div className="flex gap-2">
              <Button disabled={!!urlErr} onClick={save}>
                Save
              </Button>
              <Button variant="outline" disabled={!!urlErr} onClick={test}>
                Test connection
              </Button>
            </div>
            {result && <p className="text-sm text-muted-foreground">{result}</p>}
          </FieldGroup>
        </CardContent>
      </Card>
    </div>
  );
}
