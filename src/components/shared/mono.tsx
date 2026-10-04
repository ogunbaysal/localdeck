import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/** Monospace run for paths, ports, PIDs, URLs and commands. */
export function Mono({ className, ...props }: ComponentProps<"span">) {
  return <span data-slot="mono" className={cn("font-mono text-[0.92em]", className)} {...props} />;
}
