import type { LucideIcon } from "lucide-react";
import { useId, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/**
 * Button that explains itself when unavailable: a disabled button swallows pointer events,
 * so the tooltip hangs off a focusable wrapper and the reason is also exposed via
 * `aria-describedby`.
 */
export function ActionButton({
  label,
  icon: Icon,
  variant = "outline",
  size = "sm",
  disabled = false,
  disabledReason,
  onClick,
  children,
}: {
  label: string;
  icon?: LucideIcon;
  variant?: "default" | "outline" | "destructive" | "ghost";
  size?: "sm" | "xs";
  disabled?: boolean;
  disabledReason?: string;
  onClick: () => void;
  children?: ReactNode;
}) {
  const reasonId = useId();
  const explained = disabled && disabledReason !== undefined;
  const button = (
    <Button
      type="button"
      variant={variant}
      size={size}
      disabled={disabled}
      aria-describedby={explained ? reasonId : undefined}
      onClick={onClick}
    >
      {Icon && <Icon aria-hidden />}
      {children ?? label}
    </Button>
  );
  if (!explained) return button;
  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <span tabIndex={0} className="inline-flex rounded-md outline-none focus-visible:ring-3 focus-visible:ring-ring/50">
            {button}
          </span>
        </TooltipTrigger>
        <TooltipContent>{disabledReason}</TooltipContent>
      </Tooltip>
      <span id={reasonId} className="sr-only">
        {disabledReason}
      </span>
    </>
  );
}
