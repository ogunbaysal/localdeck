/**
 * Typed factories for tests only. Every factory returns a complete, valid object;
 * pass `overrides` to change what the test cares about. Defaults are fixed (no random
 * data), but ids repeat: pass distinct `id`/`pid`/`key` when a test needs several.
 */
import type {
  ActivityEvent,
  AppSnapshot,
  AttentionItem,
  Component,
  Metrics,
  PortInfo,
  PrimaryCommand,
  ProcessInfo,
  Project,
  ScannerStatus,
  Settings,
} from "@/types";

/** A fixed "now" (epoch ms) used by every default timestamp. */
export const FIXTURE_NOW = 1_700_000_000_000;

export function makeMetrics(overrides: Partial<Metrics> = {}): Metrics {
  return { cpuPercent: 0, memoryBytes: 0, processCount: 0, ...overrides };
}

export function makeCommand(overrides: Partial<PrimaryCommand> = {}): PrimaryCommand {
  return {
    program: "bun",
    args: ["run", "dev"],
    cwd: "/home/dev/projects/localdeck",
    label: "bun run dev",
    script: "dev",
    ...overrides,
  };
}

export function makeProcess(overrides: Partial<ProcessInfo> = {}): ProcessInfo {
  return {
    pid: 4242,
    parentPid: null,
    name: "node",
    executable: "/usr/local/bin/node",
    args: ["node", "vite"],
    cwd: "/home/dev/projects/localdeck",
    cpuUsage: 1.5,
    memoryBytes: 120 * 1024 * 1024,
    startedAt: FIXTURE_NOW - 60_000,
    accessLevel: "full",
    origin: "external",
    componentId: null,
    depth: 0,
    ...overrides,
  };
}

export function makeComponent(overrides: Partial<Component> = {}): Component {
  return {
    id: "web",
    key: "web",
    name: "Web",
    kind: "web",
    detectedKind: "web",
    kindOverridden: false,
    technology: "Vite",
    rootPid: 4242,
    processIds: [4242],
    ports: [],
    status: "running",
    origin: "external",
    command: "vite --port 5173",
    metrics: makeMetrics({ cpuPercent: 1.5, memoryBytes: 120 * 1024 * 1024, processCount: 1 }),
    ...overrides,
  };
}

export function makePort(overrides: Partial<PortInfo> = {}): PortInfo {
  const port = overrides.port ?? 3000;
  return {
    id: `tcp:${port}`,
    port,
    addresses: ["127.0.0.1"],
    transport: "tcp",
    processes: [{ pid: 4242, name: "node", startedAt: FIXTURE_NOW - 60_000 }],
    ownerVisible: true,
    ownership: "unassigned",
    projectId: null,
    componentId: null,
    http: "none",
    isHttp: false,
    url: null,
    ...overrides,
  };
}

export function makeAttention(overrides: Partial<AttentionItem> = {}): AttentionItem {
  return {
    id: "port-conflict:3000",
    kind: "portConflict",
    message: "Port 3000 is also used by another project",
    dismissible: false,
    ...overrides,
  };
}

export function makeActivity(overrides: Partial<ActivityEvent> = {}): ActivityEvent {
  return {
    id: 1,
    projectId: 1,
    kind: "projectStarted",
    metadata: { detected: false },
    message: "localdeck started",
    createdAt: FIXTURE_NOW,
    ...overrides,
  };
}

/** A stopped project with no components, processes or ports. */
export function makeProject(overrides: Partial<Project> = {}): Project {
  return {
    id: 1,
    name: "localdeck",
    detectedName: "localdeck",
    rootPath: "/home/dev/projects/localdeck",
    rootKind: "git",
    runtimeStatus: "stopped",
    stopPending: false,
    attention: [],
    stack: [],
    packageManager: null,
    components: [],
    processes: [],
    ports: [],
    primaryUrl: null,
    primaryCommand: null,
    launchProfiles: [],
    overrides: {
      displayName: null,
      primaryUrl: null,
      preferredEditor: null,
      preferredTerminal: null,
      autoRestart: false,
    },
    metrics: makeMetrics(),
    firstSeenAt: FIXTURE_NOW - 86_400_000,
    lastSeenAt: FIXTURE_NOW,
    favorite: false,
    ignored: false,
    recentActivity: [],
    ...overrides,
  };
}

/** A running project: one Vite web component on :3000 with a URL and a start command. */
export function makeRunningProject(overrides: Partial<Project> = {}): Project {
  const rootPath = overrides.rootPath ?? "/home/dev/projects/localdeck";
  return makeProject({
    runtimeStatus: "running",
    stack: [{ id: "vite", label: "Vite" }],
    packageManager: "bun",
    components: [makeComponent({ ports: [3000] })],
    processes: [makeProcess({ componentId: "web" })],
    ports: [3000],
    primaryUrl: { url: "http://localhost:3000", source: "webComponent" },
    primaryCommand: makeCommand({ cwd: rootPath }),
    metrics: makeMetrics({ cpuPercent: 4.2, memoryBytes: 286 * 1024 * 1024, processCount: 1 }),
    ...overrides,
  });
}

export function makeScanner(overrides: Partial<ScannerStatus> = {}): ScannerStatus {
  return {
    intervalMs: 2000,
    scanCount: 1,
    lastScanMs: 12,
    processAccess: { total: 100, full: 100, partial: 0, unavailable: 0 },
    socketsAvailable: true,
    socketError: null,
    socketsSkipped: 0,
    persistenceError: null,
    platform: "macos",
    ...overrides,
  };
}

export function makeSnapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return {
    projects: [],
    ports: [],
    scanner: makeScanner(),
    updatedAt: FIXTURE_NOW,
    ...overrides,
  };
}

export function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    theme: "system",
    launchAtLogin: false,
    preferredEditor: null,
    forgetAfterDays: null,
    preferredTerminal: null,
    notificationsEnabled: true,
    globalShortcut: null,
    ...overrides,
  };
}
