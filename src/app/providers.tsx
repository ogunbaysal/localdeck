import type { ReactNode } from "react";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { RuntimeBootstrap } from "./bootstrap";
import { ThemeProvider, useResolvedTheme } from "./theme";

function ThemedToaster() {
  return <Toaster theme={useResolvedTheme()} position="bottom-right" />;
}

/** Theme, tooltips, toasts and runtime bootstrap. Must live inside the Router. */
export function Providers({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider>
      <TooltipProvider delayDuration={400} skipDelayDuration={200}>
        <RuntimeBootstrap>{children}</RuntimeBootstrap>
        <ThemedToaster />
      </TooltipProvider>
    </ThemeProvider>
  );
}
