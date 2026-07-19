import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Sparkles, ThumbsDown, ThumbsUp, Minus } from "lucide-react";
import { toast } from "sonner";
import { api, type FileDiff } from "../lib/api";
import { useAuth } from "../lib/auth";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { DiffBlock } from "./DiffBlock";

export function FeedbackPanel({ skillId, isSkill }: { skillId: string; isSkill: boolean }) {
  const qc = useQueryClient();
  const { canWrite } = useAuth();
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
      toast.success("Draft ready — review below");
    },
    onError: (e) => toast.error(`Refine failed: ${(e as Error).message}`),
  });

  const merge = useMutation({
    mutationFn: () => api.refineMerge(skillId),
    onSuccess: () => {
      setDraft(null);
      reloadAll();
      toast.success("Merged into main");
    },
    onError: (e) => toast.error(`Merge failed: ${(e as Error).message}`),
  });

  const openCount = feedback.data?.filter((f) => f.status === "open").length ?? 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          Feedback
          {openCount > 0 && <Badge variant="secondary">{openCount} open</Badge>}
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
            <ToggleGroupItem value="-1" aria-label="bad">
              <ThumbsDown />
            </ToggleGroupItem>
            <ToggleGroupItem value="0" aria-label="neutral">
              <Minus />
            </ToggleGroupItem>
            <ToggleGroupItem value="1" aria-label="good">
              <ThumbsUp />
            </ToggleGroupItem>
          </ToggleGroup>
          <Textarea
            className="flex-1"
            rows={2}
            placeholder="what worked / what to fix"
            value={content}
            onChange={(e) => setContent(e.target.value)}
          />
          <Button disabled={!content.trim() || add.isPending} onClick={() => add.mutate()}>
            Send
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
              {refine.isPending ? "Refining…" : "Refine from feedback"}
            </Button>
            {draft && (
              <>
                <Button disabled={merge.isPending} onClick={() => merge.mutate()}>
                  Merge to main
                </Button>
                <Button variant="ghost" onClick={() => setDraft(null)}>
                  Discard
                </Button>
              </>
            )}
          </div>
        )}

        {draft && (
          <div>
            <div className="mb-1 text-sm text-muted-foreground">Proposed changes (refine draft)</div>
            {draft
              .filter((f) => f.status !== "unchanged")
              .map((f) => (
                <div key={f.path} className="mb-2">
                  <div className="text-sm text-muted-foreground">
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
              <Badge variant={f.rating < 0 ? "destructive" : f.rating > 0 ? "secondary" : "outline"}>
                {f.rating > 0 ? "+1" : f.rating < 0 ? "-1" : "0"}
              </Badge>
              <span className="flex-1">{f.content}</span>
              <span className="text-muted-foreground">{f.source}</span>
              <Badge variant="outline">{f.status}</Badge>
            </div>
          ))}
          {feedback.data?.length === 0 && (
            <span className="text-sm text-muted-foreground">No feedback yet.</span>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
