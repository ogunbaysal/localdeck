import { ShieldAlert } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { getLimitedAccessReasons } from "@/lib/scanner";
import { useRuntimeStore } from "@/stores/runtime-store";
import { cn } from "@/lib/utils";

/**
 * Calm notice shown when the scanner can't see everything (sockets unreadable, sockets
 * skipped, history can't be saved). Renders nothing when access is complete.
 */
export function LimitedAccessBanner({ className }: { className?: string }) {
  const scanner = useRuntimeStore((state) => state.snapshot?.scanner ?? null);
  if (!scanner) return null;
  const reasons = getLimitedAccessReasons(scanner);
  if (reasons.length === 0) return null;

  return (
    <Alert
      data-slot="limited-access-banner"
      className={cn("border-warning/40 bg-warning/10", className)}
    >
      <ShieldAlert className="text-warning" />
      <AlertTitle>Limited access</AlertTitle>
      <AlertDescription>
        <ul className="space-y-0.5">
          {reasons.map((reason) => (
            <li key={reason.id}>
              <span className="font-medium text-foreground">{reason.title}.</span> {reason.detail}
            </li>
          ))}
        </ul>
      </AlertDescription>
    </Alert>
  );
}
