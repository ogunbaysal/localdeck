import { Keyboard, X } from "lucide-react";
import { useState, type KeyboardEvent } from "react";
import { Button } from "@/components/ui/button";
import { eventToAccelerator, formatAccelerator } from "@/lib/shortcut";
import type { Platform } from "@/types";

/**
 * Click to record a shortcut. While recording, the next key combination with at least one
 * modifier is reported through `onCapture` (as a Tauri accelerator); Esc or leaving the
 * field cancels. The value shown always comes from `value`, so a rejected save reverts it.
 */
export function ShortcutField({
  id,
  value,
  platform,
  disabled,
  invalid,
  describedBy,
  onCapture,
  onClear,
}: {
  id: string;
  value: string | null;
  platform: Platform;
  disabled: boolean;
  invalid: boolean;
  describedBy?: string;
  onCapture: (accelerator: string) => void;
  onClear: () => void;
}) {
  const [recording, setRecording] = useState(false);

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    // Nothing typed while recording may reach the page (⌘K would toggle the palette).
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setRecording(false);
      return;
    }
    const accelerator = eventToAccelerator(event.nativeEvent, platform);
    if (accelerator === null) return;
    setRecording(false);
    onCapture(accelerator);
  };

  return (
    <>
      <Button
        id={id}
        type="button"
        variant="outline"
        size="sm"
        disabled={disabled}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        className="min-w-36 justify-start font-mono"
        onClick={() => setRecording(true)}
        onKeyDown={onKeyDown}
        onBlur={() => setRecording(false)}
      >
        <Keyboard aria-hidden />
        {recording ? (
          <span className="font-sans text-muted-foreground">Press a shortcut…</span>
        ) : value ? (
          formatAccelerator(value, platform)
        ) : (
          <span className="font-sans text-muted-foreground">Not set</span>
        )}
      </Button>
      {value && !recording && (
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          disabled={disabled}
          aria-label="Clear shortcut"
          title="Clear shortcut"
          onClick={onClear}
        >
          <X />
        </Button>
      )}
    </>
  );
}
