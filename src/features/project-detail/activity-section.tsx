import { memo } from "react";
import { Mono } from "@/components/shared/mono";
import { SectionHeader } from "@/components/shared/section-header";
import { formatDateTime, formatRelative } from "@/lib/format";
import type { ActivityEvent } from "@/types";

/** Recent project events, newest first (the backend already orders and bounds them). */
export const ActivitySection = memo(function ActivitySection({ events }: { events: readonly ActivityEvent[] }) {
  return (
    <section aria-label="Recent activity" className="space-y-2">
      <SectionHeader title="Recent activity" />
      {events.length === 0 ? (
        <p className="rounded-lg border border-dashed px-3 py-4 text-center text-muted-foreground">
          No activity yet
        </p>
      ) : (
        <ol className="divide-y rounded-lg border">
          {events.map((event) => (
            <li key={event.id} className="flex items-baseline gap-3 px-3 py-1.5">
              <Mono
                className="w-24 shrink-0 text-xs text-muted-foreground tabular-nums"
                title={formatRelative(event.createdAt)}
              >
                {formatDateTime(event.createdAt)}
              </Mono>
              <span className="min-w-0">{event.message}</span>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
});
