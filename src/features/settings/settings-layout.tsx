import { useId, type ReactNode } from "react";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

/** A titled group of rows separated by hairlines (no nested cards). */
export function SettingsSection({
  title,
  description,
  children,
}: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-1.5">
      <div>
        <h2
          id={headingId}
          className="text-xs font-medium tracking-wide text-muted-foreground uppercase"
        >
          {title}
        </h2>
        {description && <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>}
      </div>
      <div className="divide-y border-y">{children}</div>
    </section>
  );
}

/** Label + helper text on the left, control on the right; stacks when the window is narrow. */
export function SettingRow({
  label,
  description,
  htmlFor,
  control,
  className,
}: {
  label: string;
  description?: ReactNode;
  /** `id` of the control, so the label names it and clicking the label focuses it. */
  htmlFor?: string;
  control: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-wrap items-center justify-between gap-x-6 gap-y-2 py-2.5", className)}>
      <div className="min-w-0 flex-1 basis-60">
        <Label htmlFor={htmlFor} className="leading-snug">
          {label}
        </Label>
        {description && (
          <p className="mt-0.5 text-xs leading-snug text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2">{control}</div>
    </div>
  );
}
