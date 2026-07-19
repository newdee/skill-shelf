import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { api, type RouteResult } from "../lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Badge } from "@/components/ui/badge";
import { Empty, EmptyTitle } from "@/components/ui/empty";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";

export function RouteTester() {
  const [query, setQuery] = useState("");
  const [topK, setTopK] = useState(5);
  const [mode, setMode] = useState<"fuzzy" | "smart" | "exact">("fuzzy");
  const [results, setResults] = useState<RouteResult[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run() {
    setError(null);
    try {
      setResults(await api.route({ query, top_k: topK, mode }));
    } catch (e) {
      setError((e as Error).message);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h1 className="text-2xl font-semibold">Route</h1>
        <p className="text-sm text-muted-foreground">
          Describe a need (fuzzy) or name a skill exactly.
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">
            <ToggleGroup
              type="single"
              value={mode}
              onValueChange={(v) => v && setMode(v as "fuzzy" | "smart" | "exact")}
              variant="outline"
            >
              <ToggleGroupItem value="fuzzy">Fuzzy</ToggleGroupItem>
              <ToggleGroupItem value="smart">Smart (LLM)</ToggleGroupItem>
              <ToggleGroupItem value="exact">Exact</ToggleGroupItem>
            </ToggleGroup>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="q">{mode === "exact" ? "Skill name" : "Need"}</FieldLabel>
              <Textarea
                id="q"
                rows={mode === "exact" ? 1 : 3}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={mode === "exact" ? "pdf-parse" : "extract text from a pdf"}
              />
            </Field>
            <div className="flex items-end gap-3">
              {mode === "fuzzy" && (
                <Field className="w-28">
                  <FieldLabel htmlFor="k">top_k</FieldLabel>
                  <Input
                    id="k"
                    type="number"
                    min={1}
                    value={topK}
                    onChange={(e) => setTopK(Number(e.target.value) || 1)}
                  />
                </Field>
              )}
              <Button disabled={!query.trim()} onClick={run}>
                Route
              </Button>
            </div>
            {error && <p className="text-sm text-destructive">Failed: {error}</p>}
          </FieldGroup>
        </CardContent>
      </Card>

      {results?.map((r) => (
        <Card key={r.skill_id}>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Link to="/skill/$id" params={{ id: r.skill_id }} className="hover:underline">
                {r.name}
              </Link>
              <Badge variant="secondary">score {r.score.toFixed(4)}</Badge>
            </CardTitle>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground">{r.description}</CardContent>
        </Card>
      ))}
      {results?.length === 0 && (
        <Empty>
          <EmptyTitle>{mode === "exact" ? "No skill by that name" : "No matches"}</EmptyTitle>
        </Empty>
      )}
    </div>
  );
}
