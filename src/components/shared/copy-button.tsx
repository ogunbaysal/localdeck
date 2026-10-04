import { Check, Copy } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { copyToClipboard, runAction } from "@/lib/actions";
import { cn } from "@/lib/utils";

const RESET_MS = 1500;

/** Icon button that copies `value`; shows a check briefly, toasts on failure. */
export function CopyButton({
  value,
  label = "Copy",
  className,
}: {
  value: string;
  /** Accessible name, e.g. "Copy command". */
  label?: string;
  className?: string;
}) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const copy = async () => {
    const ok = await runAction(
      label,
      async () => {
        await copyToClipboard(value);
        return true;
      },
      { success: false },
    );
    if (!ok) return;
    setCopied(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), RESET_MS);
  };

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-xs"
      aria-label={copied ? "Copied" : label}
      title={copied ? "Copied" : label}
      className={cn("text-muted-foreground", className)}
      onClick={() => void copy()}
    >
      {copied ? <Check className="text-success" /> : <Copy />}
    </Button>
  );
}
