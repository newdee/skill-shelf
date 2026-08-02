import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Sparkles, ThumbsDown, ThumbsUp, Minus } from "lucide-react";
import { toast } from "sonner";
import { api, type FileDiff } from "../lib/api";
import { useAuth } from "../lib/auth";
import { useT } from "../lib/i18n";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { DiffBlock } from "./DiffBlock";

export function FeedbackPanel({ skillId, isSkill }: { skillId: string; isSkill: boolean }) {
  const qc = useQueryClient();
  const { canWrite } = useAuth();
  const { t } = useT();
  const feedback = useQuery({
    queryKey: ["feedback", skillId],
    queryFn: () => api.listFeedback(skillId),
  });

  const [rating, setRating] = useState("1");
  const [content, setContent] = useState("");
  const [draft, setDraft] = useState<FileDiff[] | null>(null);

  const reloadAll = () => {
    qc.invalidateQueries({ queryKey: ["feedback", skillId] });
    qc.invalidateQueries({ queryKey: ["skill", skillId] });
    qc.invalidateQueries({ queryKey: ["commits", skillId] });
  };

  const add = useMutation({
    mutationFn: () => api.addFeedback(skillId, { rating: Number(rating), content }),
    onSuccess: () => {
      setContent("");
      qc.invalidateQueries({ queryKey: ["feedback", skillId] });
    },
  });

  const refine = useMutation({
    mutationFn: () => api.refine(skillId),
    onSuccess: (r) => {
      setDraft(r.diff);
      toast.success(t("feedback.draftReady"));
    },
    onError: (e) => toast.error(t("feedback.refineFailed", { error: (e as Error).message })),
  });

  const merge = useMutation({
    mutationFn: () => api.refineMerge(skillId),
    onSuccess: () => {
      setDraft(null);
      reloadAll();
      toast.success(t("feedback.merged"));
    },
    onError: (e) => toast.error(t("feedback.mergeFailed", { error: (e as Error).message })),
  });

  const openCount = feedback.data?.filter((f) => f.status === "open").length ?? 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          {t("feedback.title")}
          {openCount > 0 && <Badge variant="secondary">{t("feedback.open", { n: openCount })}</Badge>}
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex items-start gap-2">
          <ToggleGroup
            type="single"
            value={rating}
            onValueChange={(v) => v && setRating(v)}
            variant="outline"
          >
            <ToggleGroupItem value="-1" aria-label={t("feedback.bad")}>
              <ThumbsDown />
            </ToggleGroupItem>
            <ToggleGroupItem value="0" aria-label={t("feedback.neutral")}>
              <Minus />
            </ToggleGroupItem>
            <ToggleGroupItem value="1" aria-label={t("feedback.good")}>
              <ThumbsUp />
            </ToggleGroupItem>
          </ToggleGroup>
          <Textarea
            className="flex-1"
            rows={2}
            placeholder={t("feedback.placeholder")}
            value={content}
            onChange={(e) => setContent(e.target.value)}
          />
          <Button disabled={!content.trim() || add.isPending} onClick={() => add.mutate()}>
            {t("feedback.send")}
          </Button>
        </div>

        {isSkill && canWrite && (
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              disabled={openCount === 0 || refine.isPending}
              onClick={() => refine.mutate()}
            >
              <Sparkles data-icon="inline-start" />
              {refine.isPending ? t("feedback.refining") : t("feedback.refine")}
            </Button>
            {draft && (
              <>
                <Button disabled={merge.isPending} onClick={() => merge.mutate()}>
                  {t("feedback.merge")}
                </Button>
                <Button variant="ghost" onClick={() => setDraft(null)}>
                  {t("feedback.discard")}
                </Button>
              </>
            )}
          </div>
        )}

        {draft && (
          <div>
            <div className="label-mono mb-1">{t("feedback.proposed")}</div>
            {draft
              .filter((f) => f.status !== "unchanged")
              .map((f) => (
                <div key={f.path} className="mb-2">
                  <div className="font-mono text-xs text-muted-foreground">
                    {f.status} — {f.path}
                  </div>
                  {f.diff && <DiffBlock text={f.diff} />}
                </div>
              ))}
          </div>
        )}

        <div className="flex flex-col gap-2">
          {feedback.data?.map((f) => (
            <div key={f.id} className="flex items-center gap-2 text-sm">
              <Badge
                className="font-mono"
                variant={f.rating < 0 ? "destructive" : f.rating > 0 ? "secondary" : "outline"}
              >
                {f.rating > 0 ? "+1" : f.rating < 0 ? "-1" : "0"}
              </Badge>
              <span className="flex-1">{f.content}</span>
              <span className="label-mono">{f.source}</span>
              <Badge className="font-mono" variant="outline">
                {f.status}
              </Badge>
            </div>
          ))}
          {feedback.data?.length === 0 && (
            <span className="text-sm text-muted-foreground">{t("feedback.empty")}</span>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
