import type { StartOption, StartSource } from "@/types";

/** Display order of start-option sources. */
const SOURCE_ORDER: readonly StartSource[] = [
  "packageJson",
  "makefile",
  "just",
  "composer",
  "cargo",
  "compose",
  "pyproject",
];

export const SOURCE_LABEL: Record<StartSource, string> = {
  packageJson: "Package scripts",
  makefile: "Makefile",
  just: "just",
  composer: "Composer",
  cargo: "Cargo",
  compose: "Docker Compose",
  pyproject: "Python",
};

export interface StartOptionGroup {
  source: StartSource;
  options: StartOption[];
}

/** Options grouped by source in a fixed order; empty sources are dropped. */
export function groupBySource(options: readonly StartOption[]): StartOptionGroup[] {
  return SOURCE_ORDER.map((source) => ({
    source,
    options: options.filter((option) => option.source === source),
  })).filter((group) => group.options.length > 0);
}
