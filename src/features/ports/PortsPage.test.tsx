import { afterEach, describe, expect, test } from "bun:test";
import { screen, waitFor, within } from "@testing-library/react";
import { useLocation } from "react-router";
import { toast } from "sonner";
import { useUiStore } from "@/stores/ui-store";
import { backend } from "@/test/backend";
import {
  makeComponent,
  makePort,
  makeProject,
  makeRunningProject,
  makeScanner,
  makeSnapshot,
} from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import { PortsPage } from "./PortsPage";

// Sonner keeps toasts in module state and replays them to the next <Toaster>.
afterEach(() => {
  toast.dismiss();
});

const anilarim = makeRunningProject({
  id: 1,
  name: "Anılarım",
  components: [makeComponent({ id: "web", name: "Web", ports: [3000] })],
});
const hiddenProject = makeProject({ id: 3, name: "Old Blog", ignored: true });

const web = makePort({
  port: 3000,
  ownership: "assigned",
  projectId: 1,
  componentId: "web",
  processes: [{ pid: 1234, name: "next", startedAt: 1000 }],
  http: "confirmed",
  isHttp: true,
  url: "http://localhost:3000",
});
const postgres = makePort({
  port: 5432,
  processes: [{ pid: 900, name: "postgres", startedAt: null }],
});
const ambiguous = makePort({
  port: 8080,
  ownership: "ambiguous",
  processes: [
    { pid: 11, name: "node", startedAt: null },
    { pid: 12, name: "python", startedAt: null },
  ],
});
const hidden = makePort({ port: 631, ownerVisible: false, processes: [] });
const ignored = makePort({
  port: 4000,
  ownership: "assigned",
  projectId: 3,
  processes: [{ pid: 77, name: "bun", startedAt: null }],
});
const probing = makePort({ port: 9000, http: "probing", processes: [{ pid: 5, name: "dev", startedAt: null }] });

const snapshot = makeSnapshot({
  projects: [anilarim, hiddenProject],
  ports: [postgres, web, ambiguous, hidden, ignored, probing],
});

function LocationProbe() {
  return <output aria-label="location">{useLocation().pathname}</output>;
}

function setup(overrides: Parameters<typeof makeSnapshot>[0] = {}) {
  return renderWithProviders(
    <>
      <PortsPage />
      <LocationProbe />
    </>,
    { snapshot: { ...snapshot, ...overrides }, toaster: true },
  );
}

const rowOf = (port: number) => {
  const row = screen.getByRole("button", { name: `Port ${port}` }).closest("tr");
  if (!row) throw new Error(`no row for ${port}`);
  return within(row);
};

describe("ports table", () => {
  test("lists every endpoint ordered by port with project, component, process and type", () => {
    setup();
    const order = screen
      .getAllByRole("button", { name: /^Port \d+$/ })
      .map((button) => button.textContent);
    expect(order).toEqual(["631", "3000", "4000", "5432", "8080", "9000"]);

    const row = rowOf(3000);
    expect(row.getByRole("link", { name: "Anılarım" })).toHaveAttribute("href", "/projects/1");
    expect(row.getByText("Web")).toBeInTheDocument();
    expect(row.getByText("next")).toBeInTheDocument();
    expect(row.getByText("1234")).toBeInTheDocument();
    expect(row.getByText("HTTP")).toBeInTheDocument();
  });

  test("an unassigned Postgres shows no project or component — never a guess", () => {
    setup();
    const row = rowOf(5432);
    expect(row.queryByRole("link")).not.toBeInTheDocument();
    expect(row.getByText("postgres")).toBeInTheDocument();
    expect(row.getByText("TCP")).toBeInTheDocument();
    expect(row.getAllByText("—")).toHaveLength(2);
  });

  test("ambiguous ports get an amber badge and keep every PID (never pick one)", () => {
    setup();
    const row = rowOf(8080);
    expect(row.getByText("ambiguous")).toBeInTheDocument();
    expect(row.queryByRole("link")).not.toBeInTheDocument();
    expect(row.getByText("2 processes")).toBeInTheDocument();
  });

  test("a socket whose owner the OS hides says so", () => {
    setup();
    expect(rowOf(631).getByText("Owner not visible (limited access)")).toBeInTheDocument();
  });

  test("ignored projects stay linked but are tagged, and probing ports are marked", () => {
    setup();
    expect(rowOf(4000).getByText("ignored")).toBeInTheDocument();
    expect(rowOf(4000).getByRole("link", { name: "Old Blog" })).toBeInTheDocument();
    expect(rowOf(9000).getByText("probing…")).toBeInTheDocument();
  });
});

describe("search and filters", () => {
  const visiblePorts = () =>
    screen.queryAllByRole("button", { name: /^Port \d+$/ }).map((button) => button.textContent);

  test("matches port number, :port, localhost:port, project, component and process names", async () => {
    const { user } = setup();
    const box = screen.getByRole("searchbox", { name: "Search ports" });

    await user.type(box, "5432");
    expect(visiblePorts()).toEqual(["5432"]);

    await user.clear(box);
    await user.type(box, "localhost:3000");
    expect(visiblePorts()).toEqual(["3000"]);

    await user.clear(box);
    await user.type(box, "anılar");
    expect(visiblePorts()).toEqual(["3000"]);

    await user.clear(box);
    await user.type(box, "python");
    expect(visiblePorts()).toEqual(["8080"]);

    await user.clear(box);
    await user.type(box, "web");
    expect(visiblePorts()).toEqual(["3000"]);
  });

  test("filter group narrows to HTTP, Assigned and Unassigned (ambiguous counts as unassigned)", async () => {
    const { user } = setup();
    await user.click(screen.getByRole("radio", { name: "HTTP" }));
    expect(visiblePorts()).toEqual(["3000"]);
    await user.click(screen.getByRole("radio", { name: "Assigned" }));
    expect(visiblePorts()).toEqual(["3000", "4000"]);
    await user.click(screen.getByRole("radio", { name: "Unassigned" }));
    expect(visiblePorts()).toEqual(["631", "5432", "8080", "9000"]);
    await user.click(screen.getByRole("radio", { name: "All" }));
    expect(visiblePorts()).toHaveLength(6);
  });

  test("no matches offers a way back", async () => {
    const { user } = setup();
    await user.type(screen.getByRole("searchbox", { name: "Search ports" }), "zzz");
    expect(screen.getByText("No ports match")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Clear search and filter" }));
    expect(visiblePorts()).toHaveLength(6);
  });
});

describe("what owns port N?", () => {
  const answer = () => screen.getByRole("status", { name: "Port owner" });

  test("names the project, component, process and pid", async () => {
    const { user } = setup();
    await user.type(screen.getByRole("searchbox", { name: "Search ports" }), ":3000");
    expect(answer()).toHaveTextContent("Port 3000 → Anılarım · Web (next) · pid 1234");
  });

  test("says nothing is listening when the port is free", async () => {
    const { user } = setup();
    await user.type(screen.getByRole("searchbox", { name: "Search ports" }), "3999");
    expect(answer()).toHaveTextContent("Nothing is listening on :3999");
  });

  test("an unattributed process is reported as such", async () => {
    const { user } = setup();
    await user.type(screen.getByRole("searchbox", { name: "Search ports" }), "5432");
    expect(answer()).toHaveTextContent("Port 5432 → postgres · pid 900 · not attributed to any project");
  });

  test("two processes → ambiguous, with both PIDs", async () => {
    const { user } = setup();
    await user.type(screen.getByRole("searchbox", { name: "Search ports" }), "localhost:8080");
    expect(answer()).toHaveTextContent(
      "Port 8080 is held by 2 processes — ownership ambiguous (pids 11, 12)",
    );
  });

  test("a hidden owner is not guessed", async () => {
    const { user } = setup();
    await user.type(screen.getByRole("searchbox", { name: "Search ports" }), "631");
    expect(answer()).toHaveTextContent("owner isn't visible (limited access)");
  });

  test("is absent for text queries and can't claim 'nothing' when sockets are unreadable", async () => {
    const { user } = setup({
      ports: [],
      scanner: makeScanner({ socketsAvailable: false }),
    });
    const box = screen.getByRole("searchbox", { name: "Search ports" });
    await user.type(box, "next");
    expect(screen.queryByRole("status", { name: "Port owner" })).not.toBeInTheDocument();
    await user.clear(box);
    await user.type(box, "3000");
    expect(answer()).toHaveTextContent("Can't tell what is on :3000");
  });
});

describe("port detail", () => {
  test("selecting a row reveals addresses, protocol, owners, project and URL; selecting again hides it", async () => {
    const { user } = setup({
      ports: [{ ...web, addresses: ["127.0.0.1", "::1"], processes: [web.processes[0]] }],
    });
    await user.click(screen.getByRole("button", { name: "Port 3000" }));
    const panel = screen.getByRole("region", { name: "Port 3000 details" });
    expect(useUiStore.getState().selectedPortId).toBe("tcp:3000");
    expect(panel).toHaveTextContent("127.0.0.1:3000");
    expect(panel).toHaveTextContent("[::1]:3000");
    expect(panel).toHaveTextContent("TCP · listening");
    expect(panel).toHaveTextContent("next");
    expect(panel).toHaveTextContent("pid 1234");
    expect(within(panel).getByRole("link", { name: "Anılarım" })).toHaveAttribute("href", "/projects/1");
    expect(panel).toHaveTextContent("http://localhost:3000");

    await user.click(screen.getByRole("button", { name: "Port 3000" }));
    expect(screen.queryByRole("region", { name: "Port 3000 details" })).not.toBeInTheDocument();
  });

  test("a preselected port (palette reveal) opens its panel", () => {
    useUiStore.getState().selectPort("tcp:5432");
    setup();
    expect(screen.getByRole("region", { name: "Port 5432 details" })).toBeInTheDocument();
  });

  test("Open URL asks the backend to open that port", async () => {
    backend.reply("open_port_url", undefined);
    const { user } = setup();
    await user.click(screen.getByRole("button", { name: "Port 3000" }));
    await user.click(screen.getByRole("button", { name: "Open URL" }));
    await waitFor(() => expect(backend.callsTo("open_port_url")).toEqual([{ portId: "tcp:3000" }]));
  });

  test("Reveal project navigates to the project", async () => {
    const { user } = setup();
    await user.click(screen.getByRole("button", { name: "Port 3000" }));
    await user.click(screen.getByRole("button", { name: "Reveal project" }));
    expect(screen.getByLabelText("location")).toHaveTextContent("/projects/1");
  });

  test("Terminate is immediate and passes the start time; Force Kill waits for confirmation", async () => {
    backend.reply("terminate_process", undefined);
    backend.reply("force_kill_process", undefined);
    const { user } = setup();
    await user.click(screen.getByRole("button", { name: "Port 3000" }));

    await user.click(screen.getByRole("button", { name: "Terminate next (pid 1234)" }));
    await waitFor(() =>
      expect(backend.callsTo("terminate_process")).toEqual([{ pid: 1234, startedAt: 1000 }]),
    );
    expect(backend.callsTo("force_kill_process")).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Force kill next (pid 1234)" }));
    const dialog = screen.getByRole("alertdialog", { name: /Force kill next \(pid 1234\)\?/ });
    expect(backend.callsTo("force_kill_process")).toHaveLength(0);

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(backend.callsTo("force_kill_process")).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Force kill next (pid 1234)" }));
    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", { name: "Force Kill" }),
    );
    await waitFor(() =>
      expect(backend.callsTo("force_kill_process")).toEqual([{ pid: 1234, startedAt: 1000 }]),
    );
  });

  test("every PID of an ambiguous port gets its own actions, and no project/URL actions appear", async () => {
    const { user } = setup();
    await user.click(screen.getByRole("button", { name: "Port 8080" }));
    const panel = screen.getByRole("region", { name: "Port 8080 details" });
    expect(within(panel).getByRole("button", { name: "Terminate node (pid 11)" })).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "Terminate python (pid 12)" })).toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: "Open URL" })).not.toBeInTheDocument();
    expect(within(panel).queryByRole("button", { name: "Reveal project" })).not.toBeInTheDocument();
    expect(panel).toHaveTextContent("can't prove which project");
  });

  test("a hidden owner has no terminate buttons and explains why", async () => {
    const { user } = setup();
    await user.click(screen.getByRole("button", { name: "Port 631" }));
    const panel = screen.getByRole("region", { name: "Port 631 details" });
    expect(panel).toHaveTextContent("Owner not visible (limited access)");
    expect(within(panel).queryByRole("button", { name: /Terminate/ })).not.toBeInTheDocument();
  });

  test("a failed terminate surfaces the backend's reason", async () => {
    backend.fail("terminate_process", {
      code: "PROCESS_PERMISSION_DENIED",
      message: "Operation not permitted",
    });
    const { user } = setup();
    await user.click(screen.getByRole("button", { name: "Port 3000" }));
    await user.click(screen.getByRole("button", { name: "Terminate next (pid 1234)" }));
    expect(await screen.findByText("Permission denied")).toBeInTheDocument();
    expect(screen.getByText(/Operation not permitted/)).toBeInTheDocument();
  });
});

describe("empty and limited states", () => {
  test("nothing listening", () => {
    setup({ ports: [] });
    expect(screen.getByText("Nothing is listening")).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });

  test("unreadable socket table is explained calmly with the backend's message", () => {
    setup({
      ports: [],
      scanner: makeScanner({
        socketsAvailable: false,
        socketError: { code: "PORT_SCAN_FAILED", message: "netstat2: permission denied" },
      }),
    });
    expect(screen.getByText("Listening ports unavailable")).toBeInTheDocument();
    expect(screen.getByText(/netstat2: permission denied/)).toBeInTheDocument();
    expect(screen.queryByText("Nothing is listening")).not.toBeInTheDocument();
  });
});
