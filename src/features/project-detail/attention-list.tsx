import { CircleAlert, TriangleAlert } from "lucide-react";
import { memo } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { runAction } from "@/lib/actions";
import { dismissAttention } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { AttentionItem, AttentionKind } from "@/types";

/** Failures read as errors; conflicts and exits are warnings. */
const SEVERE: ReadonlySet<AttentionKind> = new Set(["actionFailed", "runtimeLost"]);

const AttentionRow = memo(function AttentionRow({
  projectId,
  item,
}: {
  projectId: number;
  item: AttentionItem;
}) {
  const severe = SEVERE.has(item.kind);
  const Icon = severe ? CircleAlert : TriangleAlert;
  return (
    <Alert
      data-kind={item.kind}
      className={cn(
        "flex items-center gap-2 py-1.5",
        severe ? "border-destructive/30 bg-destructive/5 text-destructive" : "border-warning/40 bg-warning/10 text-warning",
      )}
    >
      <Icon aria-hidden className="size-4 shrink-0" />
      <AlertDescription className="min-w-0 flex-1 text-foreground">{item.message}</AlertDescription>
      {item.dismissible && (
        <Button
          type="button"
          variant="ghost"
          size="xs"
          className="shrink-0 text-foreground"
          onClick={() =>
            void runAction("Dismiss", () => dismissAttention(projectId, item.id), { success: false })
          }
        >
          Dismiss
        </Button>
      )}
    </Alert>
  );
});

/** Orthogonal-to-status alerts for this project; dismissible ones carry a Dismiss button. */
export function AttentionList({
  projectId,
  items,
}: {
  projectId: number;
  items: readonly AttentionItem[];
}) {
  if (items.length === 0) return null;
  return (
    <div className="space-y-1.5">
      {items.map((item) => (
        <AttentionRow key={item.id} projectId={projectId} item={item} />
      ))}
    </div>
  );
}
