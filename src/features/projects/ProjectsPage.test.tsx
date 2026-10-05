import { describe, expect, test } from "bun:test";
import { screen, within } from "@testing-library/react";
import { backend } from "@/test/backend";
import {
  makeAttention,
  makeCommand,
  makePort,
  makeProject,
  makeRunningProject,
  makeSnapshot,
} from "@/test/fixtures";
import { renderWithProviders } from "@/test/render";
import { useUiStore } from "@/stores/ui-store";
import type { Project } from "@/types";
import { ProjectsPage } from "./ProjectsPage";

const card = (name: string) => screen.getByRole("article", { name });
const queryCard = (name: string) => screen.queryByRole("article", { name });

function renderPage(projects: Project[], ports = makeSnapshot().ports) {
  return renderWithProviders(<ProjectsPage />, { snapshot: makeSnapshot({ projects, ports }) });
}

const running = (overrides: Partial<Project> = {}) => makeRunningProject({ id: 1, name: "Alpha", ...overrides });
const stopped = (overrides: Partial<Project> = {}) =>
  makeProject({ id: 2, name: "Bravo", rootPath: "/home/dev/bravo", ...overrides });

describe("project cards", () => {
  test("a running project shows status, stack, ports, metrics, path and Open + Stop", () => {
    renderPage([running({ stack: [{ id: "a", label: "Vite" }, { id: "b", label: "React" }], ports: [3000, 4000] })]);
    const view = within(card("Alpha"));
    expect(view.getByRole("link", { name: "Alpha" })).toHaveAttribute("href", "/projects/1");
    expect(view.getByText("Running")).toBeInTheDocument();
    expect(view.getByText("Vite · React")).toBeInTheDocument();
    expect(view.getByText(":3000")).toBeInTheDocument();
    expect(view.getByText(":4000")).toBeInTheDocument();
    expect(view.getByText("4.2% CPU")).toBeInTheDocument();
    expect(view.getByText("1 process")).toBeInTheDocument();
    expect(view.getByText(/\/home\/dev\/projects\/localdeck/)).toBeInTheDocument();
    expect(view.getByRole("button", { name: "Open Alpha" })).toBeInTheDocument();
    expect(view.getByRole("button", { name: "Stop Alpha" })).toBeInTheDocument();
    expect(view.queryByRole("button", { name: /^Start/ })).not.toBeInTheDocument();
  });

  test("a partial project reads Partial and still offers Open", () => {
    renderPage([running({ runtimeStatus: "partial" })]);
    expect(within(card("Alpha")).getByText("Partial")).toBeInTheDocument();
    expect(within(card("Alpha")).getByRole("button", { name: "Open Alpha" })).toBeInTheDocument();
  });

  test("a stopped remembered project hides metrics and ports and offers Start", () => {
    renderPage([running(), stopped({ primaryCommand: makeCommand() })]);
    const view = within(card("Bravo"));
    expect(view.getByText("Stopped")).toBeInTheDocument();
    expect(view.queryByText(/CPU/)).not.toBeInTheDocument();
    expect(view.getByRole("button", { name: "Start Bravo" })).toBeInTheDocument();
    expect(view.queryByRole("button", { name: /^Stop/ })).not.toBeInTheDocument();
  });

  test("a stopped project without a start command falls back to Details", () => {
    renderPage([running(), stopped()]);
    const details = within(card("Bravo")).getByRole("link", { name: "Details for Bravo" });
    expect(details).toHaveAttribute("href", "/projects/2");
  });

  test("a running project without a URL makes Stop the primary action", () => {
    renderPage([running({ primaryUrl: null })]);
    const view = within(card("Alpha"));
    expect(view.queryByRole("button", { name: /^Open/ })).not.toBeInTheDocument();
    expect(view.getByRole("button", { name: "Stop Alpha" })).toBeInTheDocument();
  });

  test("a pending stop reads Stopping… and offers Force Stop instead of Stop", () => {
    renderPage([running({ stopPending: true })]);
    const view = within(card("Alpha"));
    expect(view.getByText("Stopping…")).toBeInTheDocument();
    expect(view.getByRole("button", { name: "Force Stop Alpha" })).toBeInTheDocument();
    expect(view.queryByRole("button", { name: "Stop Alpha" })).not.toBeInTheDocument();
  });

  test("attention shows the first message and a marker, even on a stopped project", () => {
    renderPage([
      running(),
      stopped({ attention: [makeAttention({ message: "Port 3000 is taken" })] }),
    ]);
    const view = within(card("Bravo"));
    expect(view.getByText("Port 3000 is taken")).toBeInTheDocument();
    expect(view.getByRole("img", { name: "Needs attention" })).toBeInTheDocument();
    expect(view.queryByRole("button", { name: /Dismiss/ })).not.toBeInTheDocument();
  });

  test("a dismissible attention item can be dismissed through the backend", async () => {
    backend.reply("dismiss_attention", null);
    const { user } = renderPage([
      running({ attention: [makeAttention({ id: "crash:web", message: "web exited", dismissible: true })] }),
    ]);
    await user.click(within(card("Alpha")).getByRole("button", { name: "Dismiss: web exited" }));
    expect(backend.callsTo("dismiss_attention")).toEqual([{ projectId: 1, attentionId: "crash:web" }]);
  });
});

describe("primary actions", () => {
  test("Open calls open_project_url", async () => {
    backend.reply("open_project_url", null);
    const { user } = renderPage([running()]);
    await user.click(screen.getByRole("button", { name: "Open Alpha" }));
    expect(backend.callsTo("open_project_url")).toEqual([{ projectId: 1 }]);
  });

  test("Stop calls stop_project", async () => {
    backend.reply("stop_project", { signaled: [4242], failed: [] });
    const { user } = renderPage([running()]);
    await user.click(screen.getByRole("button", { name: "Stop Alpha" }));
    expect(backend.callsTo("stop_project")).toEqual([{ projectId: 1 }]);
  });

  test("Start calls start_project", async () => {
    backend.reply("start_project", null);
    const { user } = renderPage([running(), stopped({ primaryCommand: makeCommand() })]);
    await user.click(screen.getByRole("button", { name: "Start Bravo" }));
    expect(backend.callsTo("start_project")).toEqual([{ projectId: 2, profileId: null, port: null }]);
  });

  test("the star toggles the favourite through the backend", async () => {
    backend.reply("set_project_favorite", null);
    const { user } = renderPage([running(), stopped({ favorite: true })]);
    await user.click(within(card("Alpha")).getByRole("button", { name: "Pin Alpha" }));
    await user.click(within(card("Bravo")).getByRole("button", { name: "Unpin Bravo" }));
    expect(backend.callsTo("set_project_favorite")).toEqual([
      { projectId: 1, favorite: true },
      { projectId: 2, favorite: false },
    ]);
  });
});

describe("Force Stop", () => {
  test("needs explicit confirmation before the backend is called", async () => {
    backend.reply("force_stop_project", { signaled: [4242], failed: [] });
    const { user } = renderPage([running({ stopPending: true })]);

    await user.click(screen.getByRole("button", { name: "Force Stop Alpha" }));
    expect(screen.getByRole("alertdialog", { name: "Force stop Alpha?" })).toBeInTheDocument();
    expect(backend.callsTo("force_stop_project")).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(backend.callsTo("force_stop_project")).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Force Stop Alpha" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Force Stop" }));
    expect(backend.callsTo("force_stop_project")).toEqual([{ projectId: 1 }]);
  });

  test("Forget from the overflow menu is confirmed too", async () => {
    backend.reply("forget_project", null);
    const { user } = renderPage([running(), stopped()]);
    await user.click(within(card("Bravo")).getByRole("button", { name: "More actions for Bravo" }));
    await user.click(screen.getByRole("menuitem", { name: "Forget" }));
    expect(backend.callsTo("forget_project")).toHaveLength(0);
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Forget" }));
    expect(backend.callsTo("forget_project")).toEqual([{ projectId: 2 }]);
  });
});

describe("overflow menu", () => {
  test("lists the project actions with reasons for the unavailable ones", async () => {
    const { user } = renderPage([running(), stopped()]);
    await user.click(within(card("Bravo")).getByRole("button", { name: "More actions for Bravo" }));

    expect(screen.getByRole("menuitem", { name: "Details" })).toHaveAttribute("href", "/projects/2");
    const start = screen.getByRole("menuitem", { name: /^Start/ });
    expect(start).toHaveAttribute("aria-disabled", "true");
    expect(start).toHaveTextContent("No start command configured");
    expect(screen.getByRole("menuitem", { name: /^Restart/ })).toHaveTextContent("Not running");
    expect(screen.getByRole("menuitem", { name: /^Open URL/ })).toHaveTextContent("No URL detected yet");
    expect(screen.getByRole("menuitem", { name: "Open Folder" })).not.toHaveAttribute("aria-disabled");
    expect(screen.getByRole("menuitem", { name: "Forget" })).not.toHaveAttribute("aria-disabled");
    expect(screen.queryByRole("menuitem", { name: /Force Stop/ })).not.toBeInTheDocument();
  });

  test("a menu item runs its backend command", async () => {
    backend.reply("open_project_folder", null);
    const { user } = renderPage([running()]);
    await user.click(screen.getByRole("button", { name: "More actions for Alpha" }));
    await user.click(screen.getByRole("menuitem", { name: "Open Folder" }));
    expect(backend.callsTo("open_project_folder")).toEqual([{ projectId: 1 }]);
  });

  test("a running project cannot be forgotten", async () => {
    const { user } = renderPage([running()]);
    await user.click(screen.getByRole("button", { name: "More actions for Alpha" }));
    expect(screen.getByRole("menuitem", { name: /^Forget/ })).toHaveTextContent("Stop the project first");
  });
});

describe("sections and ordering", () => {
  test("ignored projects are hidden everywhere", () => {
    renderPage([running(), stopped({ ignored: true })]);
    expect(queryCard("Bravo")).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Stopped" })).not.toBeInTheDocument();
  });

  test("running and stopped projects get counted sections, favourites first then by name", () => {
    renderPage([
      running({ id: 1, name: "zeta" }),
      running({ id: 2, name: "Beta" }),
      running({ id: 3, name: "alpha" }),
      running({ id: 4, name: "Omega", favorite: true }),
      stopped({ id: 5, name: "Stopped One" }),
    ]);
    const runningSection = screen.getByRole("region", { name: "Running" });
    expect(within(runningSection).getByRole("heading", { name: /Running\s*4/ })).toBeInTheDocument();
    const names = within(runningSection)
      .getAllByRole("article")
      .map((article) => article.getAttribute("aria-label"));
    expect(names).toEqual(["Omega", "alpha", "Beta", "zeta"]);
    const stoppedSection = screen.getByRole("region", { name: "Stopped" });
    expect(within(stoppedSection).getByRole("heading", { name: /Stopped\s*1/ })).toBeInTheDocument();
    expect(within(stoppedSection).getByRole("article", { name: "Stopped One" })).toBeInTheDocument();
  });
});

describe("search and filters", () => {
  const projects = () => [
    running({ id: 1, name: "Alpha", ports: [3000] }),
    running({
      id: 2,
      name: "Beta",
      ports: [5173],
      rootPath: "/srv/beta",
      primaryUrl: { url: "http://localhost:5173", source: "webComponent" },
    }),
    stopped({ id: 3, name: "Gamma" }),
  ];

  test("typing matches by name", async () => {
    const { user } = renderPage(projects());
    await user.type(screen.getByRole("textbox", { name: "Search projects" }), "beta");
    expect(queryCard("Beta")).toBeInTheDocument();
    expect(queryCard("Alpha")).not.toBeInTheDocument();
    expect(queryCard("Gamma")).not.toBeInTheDocument();
  });

  test.each([["3000"], [":3000"], ["localhost:3000"]])("typing %p matches the project on that port", async (text) => {
    const { user } = renderPage(projects());
    await user.type(screen.getByRole("textbox", { name: "Search projects" }), text);
    expect(queryCard("Alpha")).toBeInTheDocument();
    expect(queryCard("Beta")).not.toBeInTheDocument();
  });

  test("ports owned through the port table count as well", async () => {
    const { user } = renderPage(
      [running({ id: 1, name: "Alpha", ports: [] })],
      [makePort({ port: 8088, ownership: "assigned", projectId: 1 })],
    );
    await user.type(screen.getByRole("textbox", { name: "Search projects" }), ":8088");
    expect(queryCard("Alpha")).toBeInTheDocument();
  });

  test("status chips filter by running / stopped / attention", async () => {
    const list = [...projects().slice(0, 2), stopped({ id: 3, name: "Gamma", attention: [makeAttention()] })];
    const { user } = renderPage(list);

    await user.click(screen.getByRole("radio", { name: "Stopped" }));
    expect(queryCard("Gamma")).toBeInTheDocument();
    expect(queryCard("Alpha")).not.toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "Running" }));
    expect(queryCard("Alpha")).toBeInTheDocument();
    expect(queryCard("Gamma")).not.toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "Attention" }));
    expect(screen.getByRole("region", { name: "Needs attention" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Running" })).not.toBeInTheDocument();
    expect(queryCard("Gamma")).toBeInTheDocument();
    expect(queryCard("Alpha")).not.toBeInTheDocument();

    // Re-clicking the active chip keeps a filter selected.
    await user.click(screen.getByRole("radio", { name: "Attention" }));
    expect(screen.getByRole("radio", { name: "Attention" })).toBeChecked();
  });

  test("no results explains itself and Clear filters restores the list", async () => {
    const { user } = renderPage(projects());
    await user.type(screen.getByRole("textbox", { name: "Search projects" }), "nothing-like-this");
    expect(screen.getByText("No matching projects")).toBeInTheDocument();
    expect(screen.queryByText("No running projects")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(screen.getByRole("textbox", { name: "Search projects" })).toHaveValue("");
    expect(queryCard("Alpha")).toBeInTheDocument();
    expect(useUiStore.getState().statusFilter).toBe("all");
  });

  test("the clear button and Escape empty the search", async () => {
    const { user } = renderPage(projects());
    const input = screen.getByRole("textbox", { name: "Search projects" });
    await user.type(input, "alp");
    await user.click(screen.getByRole("button", { name: "Clear search" }));
    expect(input).toHaveValue("");
    await user.type(input, "bet{Escape}");
    expect(input).toHaveValue("");
    expect(useUiStore.getState().searchQuery).toBe("");
  });
});

describe("empty states", () => {
  test("with no projects at all only the message is shown", () => {
    renderPage([]);
    expect(screen.getByText("No running projects")).toBeInTheDocument();
    expect(screen.getByText(/LocalDeck will detect it automatically/)).toBeInTheDocument();
    expect(screen.queryByText("Previously discovered")).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Search projects" })).not.toBeInTheDocument();
  });

  test("nothing running lists remembered projects; rows open details and Start works", async () => {
    backend.reply("start_project", null);
    const { user } = renderPage([
      stopped({ id: 2, name: "Bravo", primaryCommand: makeCommand() }),
      stopped({ id: 3, name: "Charlie" }),
    ]);
    expect(screen.getByText("No running projects")).toBeInTheDocument();
    const list = screen.getByRole("list", { name: "Previously discovered projects" });
    expect(within(list).getByRole("link", { name: "Bravo" })).toHaveAttribute("href", "/projects/2");
    expect(within(list).getByRole("link", { name: "Charlie" })).toHaveAttribute("href", "/projects/3");
    expect(within(list).queryByRole("button", { name: "Start Charlie" })).not.toBeInTheDocument();

    await user.click(within(list).getByRole("button", { name: "Start Bravo" }));
    expect(backend.callsTo("start_project")).toEqual([{ projectId: 2, profileId: null, port: null }]);
  });

  test("ignored projects are not remembered rows but are mentioned", () => {
    renderPage([stopped({ ignored: true })]);
    expect(screen.queryByRole("list", { name: "Previously discovered projects" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Settings" })).toHaveAttribute("href", "/settings");
  });

  test("renders nothing before the first snapshot arrives", () => {
    const { container } = renderWithProviders(<ProjectsPage />, { snapshot: null });
    expect(container).toBeEmptyDOMElement();
  });
});
