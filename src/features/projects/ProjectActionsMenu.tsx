import { Ellipsis, Info } from "lucide-react";
import { Fragment } from "react";
import { Link } from "react-router";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { ProjectAction, ProjectActionGroup } from "@/lib/project-actions";

const GROUP_ORDER: readonly ProjectActionGroup[] = ["open", "lifecycle", "manage"];

/**
 * Compact `···` overflow menu built from the shared project actions, grouped
 * open / lifecycle / manage. Unavailable actions stay visible and spell out why.
 */
export function ProjectActionsMenu({
  projectId,
  projectName,
  actions,
  onSelect,
}: {
  projectId: number;
  projectName: string;
  actions: readonly ProjectAction[];
  onSelect: (action: ProjectAction) => void;
}) {
  const groups = GROUP_ORDER.map((group) => actions.filter((action) => action.group === group)).filter(
    (items) => items.length > 0,
  );

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={`More actions for ${projectName}`}
          className="text-muted-foreground"
        >
          <Ellipsis />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuItem asChild>
          <Link to={`/projects/${projectId}`}>
            <Info />
            Details
          </Link>
        </DropdownMenuItem>
        {groups.map((items) => (
          <Fragment key={items[0].group}>
            <DropdownMenuSeparator />
            {items.map((action) => (
              <DropdownMenuItem
                key={action.id}
                variant={action.destructive ? "destructive" : "default"}
                disabled={action.disabled}
                onSelect={() => onSelect(action)}
              >
                <action.icon />
                <span className="flex min-w-0 flex-col">
                  <span className="truncate">{action.label}</span>
                  {action.disabled && action.disabledReason && (
                    <span className="truncate text-xs font-normal text-muted-foreground">
                      {action.disabledReason}
                    </span>
                  )}
                </span>
              </DropdownMenuItem>
            ))}
          </Fragment>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
