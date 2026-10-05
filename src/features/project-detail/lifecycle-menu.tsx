import { ChevronDown, Play, RotateCw } from "lucide-react";
import { Mono } from "@/components/shared/mono";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { ProjectAction } from "@/lib/project-actions";
import type { Project } from "@/types";

export type LifecycleMode = "start" | "restart";

/**
 * The chevron half of the header's Start / Restart split button: the primary command, each
 * launch profile, and the "on a free port" variants of both.
 */
export function LifecycleMenu({
  mode,
  project,
  actions,
  onInvoke,
  onFreePort,
}: {
  mode: LifecycleMode;
  project: Project;
  actions: readonly ProjectAction[];
  onInvoke: (action: ProjectAction) => void;
  /** Opens the free-port dialog for the primary command (`null`) or one profile. */
  onFreePort: (profileId: string | null) => void;
}) {
  const verb = mode === "start" ? "Start" : "Restart";
  const Icon = mode === "start" ? Play : RotateCw;
  const primary = actions.find((action) => action.id === mode);
  const profileActions = actions.filter(
    (action) => action.profileId !== undefined && action.id.startsWith(`${mode}-profile:`),
  );
  const primaryCommand = project.primaryCommand;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button type="button" variant="outline" size="icon-sm" aria-label={`${verb} options`}>
          <ChevronDown aria-hidden />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
        <DropdownMenuLabel className="text-xs font-medium text-muted-foreground">{verb}</DropdownMenuLabel>
        <DropdownMenuItem
          disabled={primary?.disabled ?? true}
          title={primary?.disabled ? primary.disabledReason : undefined}
          onSelect={() => primary && onInvoke(primary)}
        >
          <Icon aria-hidden />
          <span className="min-w-0 flex-1 truncate">
            Primary command
            {primaryCommand && <Mono className="ml-1.5 text-xs text-muted-foreground">{primaryCommand.label}</Mono>}
          </span>
        </DropdownMenuItem>
        {profileActions.map((action) => (
          <DropdownMenuItem
            key={action.id}
            disabled={action.disabled}
            title={action.disabled ? action.disabledReason : undefined}
            onSelect={() => onInvoke(action)}
          >
            <Icon aria-hidden />
            <span className="min-w-0 flex-1 truncate">{action.label}</span>
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-xs font-medium text-muted-foreground">On a free port…</DropdownMenuLabel>
        <DropdownMenuItem disabled={primary?.disabled ?? true} onSelect={() => onFreePort(null)}>
          <Icon aria-hidden />
          <span className="min-w-0 flex-1 truncate">{verb} primary command on a free port…</span>
        </DropdownMenuItem>
        {profileActions.map((action) => (
          <DropdownMenuItem
            key={`port-${action.id}`}
            disabled={action.disabled}
            onSelect={() => action.profileId !== undefined && onFreePort(action.profileId)}
          >
            <Icon aria-hidden />
            <span className="min-w-0 flex-1 truncate">{action.label} on a free port…</span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
