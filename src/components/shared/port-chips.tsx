import { cn } from "@/lib/utils";
import { formatPort } from "@/lib/format";

const chipClass =
  "inline-flex h-5 items-center rounded-md border bg-muted/50 px-1.5 font-mono text-xs tabular-nums text-foreground";

/**
 * Listening ports as compact `:3000` chips. `max` collapses the rest into `+N`
 * (the hidden ports stay available via the chip's title). With `onPortClick`
 * the chips become buttons.
 */
export function PortChips({
  ports,
  max = 4,
  onPortClick,
  className,
}: {
  ports: readonly number[];
  max?: number;
  onPortClick?: (port: number) => void;
  className?: string;
}) {
  if (ports.length === 0) return null;
  const shown = ports.slice(0, max);
  const hidden = ports.slice(max);
  return (
    <span data-slot="port-chips" className={cn("inline-flex flex-wrap items-center gap-1", className)}>
      {shown.map((port) =>
        onPortClick ? (
          <button
            key={port}
            type="button"
            onClick={() => onPortClick(port)}
            title={`Open localhost${formatPort(port)}`}
            className={cn(chipClass, "hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring/50 outline-none")}
          >
            {formatPort(port)}
          </button>
        ) : (
          <span key={port} className={chipClass}>
            {formatPort(port)}
          </span>
        ),
      )}
      {hidden.length > 0 && (
        <span className={cn(chipClass, "text-muted-foreground")} title={hidden.map(formatPort).join(" ")}>
          +{hidden.length}
        </span>
      )}
    </span>
  );
}
