import { describe, expect, test } from "bun:test";
import { makePort, makeProject, makeScanner, makeSnapshot } from "@/test/fixtures";
import {
  selectIgnoredProjects,
  selectPortsForProject,
  selectProjectById,
  selectProjects,
  selectVisibleProjects,
  useRuntimeStore,
} from "./runtime-store";

const alpha = makeProject({ id: 1, name: "alpha" });
const beta = makeProject({ id: 2, name: "beta", ignored: true });
const gamma = makeProject({ id: 3, name: "gamma", runtimeStatus: "running" });

function load(projects = [alpha, beta, gamma], updatedAt = 1000) {
  useRuntimeStore.getState().setSnapshot(makeSnapshot({ projects, updatedAt }));
}

describe("project selectors", () => {
  test("ignored projects are filtered from the visible list and listed separately", () => {
    load();
    const state = useRuntimeStore.getState();
    expect(selectProjects(state).map((p) => p.name)).toEqual(["alpha", "beta", "gamma"]);
    expect(selectVisibleProjects(state).map((p) => p.name)).toEqual(["alpha", "gamma"]);
    expect(selectIgnoredProjects(state).map((p) => p.name)).toEqual(["beta"]);
  });

  test("before the first snapshot every selector yields stable empty values", () => {
    const state = useRuntimeStore.getState();
    expect(selectVisibleProjects(state)).toHaveLength(0);
    expect(selectVisibleProjects(state)).toBe(selectVisibleProjects(state));
    expect(selectProjectById(1)(state)).toBeUndefined();
    expect(selectPortsForProject(1)(state)).toBe(selectPortsForProject(2)(state));
  });

  test("selectProjectById finds ignored projects too and tolerates missing ids", () => {
    load();
    const state = useRuntimeStore.getState();
    expect(selectProjectById(2)(state)?.name).toBe("beta");
    expect(selectProjectById(99)(state)).toBeUndefined();
    expect(selectProjectById(null)(state)).toBeUndefined();
    expect(selectProjectById(undefined)(state)).toBeUndefined();
  });

  test("derived lists keep their identity across ticks that change nothing in the projects", () => {
    load();
    const before = selectVisibleProjects(useRuntimeStore.getState());
    const projectBefore = selectProjectById(3)(useRuntimeStore.getState());

    // Next 2 s tick: brand-new JSON objects, only the timestamp and scanner counters move.
    useRuntimeStore.getState().setSnapshot(
      makeSnapshot({
        projects: [
          makeProject({ id: 1, name: "alpha" }),
          makeProject({ id: 2, name: "beta", ignored: true }),
          makeProject({ id: 3, name: "gamma", runtimeStatus: "running" }),
        ],
        updatedAt: 5000,
        scanner: makeScanner({ scanCount: 9 }),
      }),
    );
    const state = useRuntimeStore.getState();
    expect(state.snapshot?.updatedAt).toBe(5000);
    expect(selectVisibleProjects(state)).toBe(before);
    expect(selectProjectById(3)(state)).toBe(projectBefore);
  });

  test("changing one project only replaces that project", () => {
    load();
    const alphaBefore = selectProjectById(1)(useRuntimeStore.getState());
    const gammaBefore = selectProjectById(3)(useRuntimeStore.getState());
    load([alpha, beta, { ...gamma, metrics: { cpuPercent: 9, memoryBytes: 1, processCount: 1 } }], 2000);
    const state = useRuntimeStore.getState();
    expect(selectProjectById(1)(state)).toBe(alphaBefore);
    expect(selectProjectById(3)(state)).not.toBe(gammaBefore);
    expect(selectProjectById(3)(state)?.metrics.cpuPercent).toBe(9);
  });
});

describe("selectPortsForProject", () => {
  test("groups ports by owning project and leaves unassigned rows out", () => {
    const ports = [
      makePort({ port: 3000, projectId: 1, ownership: "assigned" }),
      makePort({ port: 3001, projectId: 1, ownership: "shared" }),
      makePort({ port: 5432, projectId: null }),
      makePort({ port: 8080, projectId: 3, ownership: "assigned" }),
    ];
    useRuntimeStore.getState().setSnapshot(makeSnapshot({ projects: [alpha, gamma], ports }));
    const state = useRuntimeStore.getState();
    expect(selectPortsForProject(1)(state).map((p) => p.port)).toEqual([3000, 3001]);
    expect(selectPortsForProject(3)(state).map((p) => p.port)).toEqual([8080]);
    expect(selectPortsForProject(2)(state)).toHaveLength(0);
    expect(selectPortsForProject(1)(state)).toBe(selectPortsForProject(1)(state));
  });
});

describe("snapshot and status transitions", () => {
  test("a snapshot older than the current one is ignored", () => {
    load([alpha], 2000);
    load([alpha, gamma], 1500);
    expect(useRuntimeStore.getState().snapshot?.projects).toHaveLength(1);
    load([alpha, gamma], 2000);
    expect(useRuntimeStore.getState().snapshot?.projects).toHaveLength(2);
  });

  test("starts loading; error carries the payload; ready clears it", () => {
    const store = useRuntimeStore.getState();
    expect(store.status).toBe("loading");
    store.setError({ code: "DATABASE_ERROR", message: "Database error: locked" });
    expect(useRuntimeStore.getState()).toMatchObject({
      status: "error",
      error: { code: "DATABASE_ERROR", message: "Database error: locked" },
    });
    store.setLoading();
    expect(useRuntimeStore.getState()).toMatchObject({ status: "loading", error: null });
    store.setError({ code: "DATABASE_ERROR", message: "x" });
    store.setReady();
    expect(useRuntimeStore.getState()).toMatchObject({ status: "ready", error: null });
  });
});
