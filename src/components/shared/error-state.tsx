import { CircleAlert } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { describeError, IPC_ERROR_ORIGIN, toErrorPayload } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { Mono } from "./mono";

/**
 * Renders any backend error (`ErrorPayload` or whatever was thrown) with a code-specific
 * title, the backend's message, a hint and the structured context. `onRetry` adds a button.
 */
export function ErrorState({
  error,
  onRetry,
  retryLabel = "Try again",
  actions,
  className,
}: {
  error: unknown;
  onRetry?: () => void;
  retryLabel?: string;
  actions?: ReactNode;
  className?: string;
}) {
  const payload = toErrorPayload(error);
  const { title, message, hint } = describeError(payload);
  const context = Object.entries(payload.context ?? {}).filter(
    ([key, value]) => !(key === "origin" && value === IPC_ERROR_ORIGIN),
  );

  return (
    <div
      role="alert"
      data-slot="error-state"
      data-code={payload.code}
      className={cn("flex flex-col items-center gap-1 px-4 py-10 text-center", className)}
    >
      <CircleAlert aria-hidden className="mb-1 size-6 text-destructive" />
      <h2 className="text-base font-medium">{title}</h2>
      <p className="max-w-md text-muted-foreground">{message}</p>
      {hint && <p className="max-w-md text-xs text-muted-foreground">{hint}</p>}
      {context.length > 0 && (
        <dl className="mt-2 grid max-w-md grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-left text-xs">
          {context.map(([key, value]) => (
            <div key={key} className="contents">
              <dt className="text-muted-foreground">{key}</dt>
              <dd className="min-w-0 break-all">
                <Mono>{value}</Mono>
              </dd>
            </div>
          ))}
        </dl>
      )}
      {(onRetry || actions) && (
        <div className="mt-3 flex items-center gap-2">
          {onRetry && (
            <Button variant="outline" onClick={onRetry}>
              {retryLabel}
            </Button>
          )}
          {actions}
        </div>
      )}
    </div>
  );
}
