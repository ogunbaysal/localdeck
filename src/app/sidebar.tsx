import { History, LayoutGrid, Network, PanelLeftClose, PanelLeftOpen, Settings, type LucideIcon } from "lucide-react";
import { Link, useLocation } from "react-router";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { selectVisibleProjects, useRuntimeStore } from "@/stores/runtime-store";
import { useUiStore } from "@/stores/ui-store";

interface NavItemProps {
  to: string;
  label: string;
  icon: LucideIcon;
  active: boolean;
  collapsed: boolean;
  /** Small mono counter shown after the label. */
  badge?: number;
}

function NavItem({ to, label, icon: Icon, active, collapsed, badge }: NavItemProps) {
  const link = (
    <Link
      to={to}
      aria-label={collapsed ? label : undefined}
      aria-current={active ? "page" : undefined}
      className={cn(
        "relative flex h-8 items-center gap-2 rounded-md px-2 text-sm text-muted-foreground outline-none hover:bg-sidebar-accent hover:text-sidebar-accent-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring/50",
        active && "bg-sidebar-accent font-medium text-sidebar-accent-foreground",
        collapsed && "justify-center px-0",
      )}
    >
      <Icon aria-hidden className="size-4 shrink-0" />
      {!collapsed && <span className="min-w-0 flex-1 truncate">{label}</span>}
      {badge !== undefined && badge > 0 && !collapsed && (
        <span className="font-mono text-xs tabular-nums text-muted-foreground">{badge}</span>
      )}
      {badge !== undefined && badge > 0 && collapsed && (
        <span aria-hidden className="absolute top-1 right-1 size-1.5 rounded-full bg-success" />
      )}
    </Link>
  );
  if (!collapsed) return link;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{link}</TooltipTrigger>
      <TooltipContent side="right">
        {label}
        {badge ? ` · ${badge} running` : ""}
      </TooltipContent>
    </Tooltip>
  );
}

/** Left navigation: Projects, Ports, Activity, ── , Settings. Icon-only when `collapsed`. */
export function Sidebar({ collapsed, autoCollapsed }: { collapsed: boolean; autoCollapsed: boolean }) {
  const { pathname } = useLocation();
  const runningCount = useRuntimeStore(
    (state) => selectVisibleProjects(state).filter((project) => project.runtimeStatus !== "stopped").length,
  );
  const setSidebarCollapsed = useUiStore((state) => state.setSidebarCollapsed);

  const projectsActive = pathname === "/" || pathname.startsWith("/projects/");

  return (
    <nav
      aria-label="Primary"
      className={cn(
        "flex shrink-0 flex-col gap-0.5 border-r bg-sidebar p-1.5",
        collapsed ? "w-12" : "w-48",
      )}
    >
      <NavItem to="/" label="Projects" icon={LayoutGrid} active={projectsActive} collapsed={collapsed} badge={runningCount} />
      <NavItem to="/ports" label="Ports" icon={Network} active={pathname === "/ports"} collapsed={collapsed} />
      <NavItem to="/activity" label="Activity" icon={History} active={pathname === "/activity"} collapsed={collapsed} />
      <Separator className="my-1.5" />
      <NavItem to="/settings" label="Settings" icon={Settings} active={pathname === "/settings"} collapsed={collapsed} />

      {!autoCollapsed && (
        <Button
          variant="ghost"
          size="icon-sm"
          className={cn("mt-auto text-muted-foreground", collapsed ? "self-center" : "self-start")}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          onClick={() => setSidebarCollapsed(!collapsed)}
        >
          {collapsed ? <PanelLeftOpen /> : <PanelLeftClose />}
        </Button>
      )}
    </nav>
  );
}
