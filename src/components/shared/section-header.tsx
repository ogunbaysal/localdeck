import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** Compact section heading with optional count, description and trailing actions. */
export function SectionHeader({
  title,
  count,
  description,
  actions,
  as: Heading = "h2",
  className,
}: {
  title: string;
  count?: number;
  description?: ReactNode;
  actions?: ReactNode;
  as?: "h1" | "h2" | "h3";
  className?: string;
}) {
  return (
    <div data-slot="section-header" className={cn("flex items-end justify-between gap-3", className)}>
      <div className="min-w-0">
        <Heading className="flex items-baseline gap-1.5 text-xs font-medium tracking-wide text-muted-foreground uppercase">
          {title}
          {count !== undefined && (
            <span className="font-mono tabular-nums normal-case">{count}</span>
          )}
        </Heading>
        {description && <p className="mt-0.5 text-muted-foreground">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  );
}
