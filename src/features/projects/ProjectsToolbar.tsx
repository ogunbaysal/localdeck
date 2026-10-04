import { Search, X } from "lucide-react";
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { StatusFilter } from "@/lib/search";

const FILTERS: readonly { value: StatusFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "running", label: "Running" },
  { value: "stopped", label: "Stopped" },
  { value: "attention", label: "Attention" },
];

function isStatusFilter(value: string): value is StatusFilter {
  return FILTERS.some((filter) => filter.value === value);
}

/** Search field (name, path, ports, stack) and the All / Running / Stopped / Attention chips. */
export function ProjectsToolbar({
  query,
  onQueryChange,
  filter,
  onFilterChange,
}: {
  query: string;
  onQueryChange: (query: string) => void;
  filter: StatusFilter;
  onFilterChange: (filter: StatusFilter) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <InputGroup className="min-w-48 flex-1 basis-64 sm:max-w-sm">
        <InputGroupAddon>
          <Search aria-hidden />
        </InputGroupAddon>
        <InputGroupInput
          type="text"
          value={query}
          placeholder="Search name, path or port"
          aria-label="Search projects"
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => onQueryChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape" && query !== "") {
              event.preventDefault();
              onQueryChange("");
            }
          }}
        />
        {query !== "" && (
          <InputGroupAddon align="inline-end">
            <InputGroupButton
              size="icon-xs"
              aria-label="Clear search"
              title="Clear search"
              onClick={() => onQueryChange("")}
            >
              <X />
            </InputGroupButton>
          </InputGroupAddon>
        )}
      </InputGroup>
      <ToggleGroup
        type="single"
        variant="outline"
        size="sm"
        spacing={0}
        aria-label="Filter by status"
        value={filter}
        onValueChange={(value) => {
          // Radix reports "" when the active chip is clicked again; a filter is always active.
          if (isStatusFilter(value)) onFilterChange(value);
        }}
      >
        {FILTERS.map(({ value, label }) => (
          <ToggleGroupItem key={value} value={value} className="text-xs">
            {label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </div>
  );
}
