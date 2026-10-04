import { describe, expect, test } from "bun:test";
import { makeAttention, makeComponent, makePort, makeProject } from "@/test/fixtures";
import { matchesStatusFilter, matchProject } from "./search";

const localdeck = makeProject({
  id: 1,
  name: "LocalDeck",
  rootPath: "/home/dev/projects/localdeck",
  ports: [1420],
  stack: [
    { id: "react", label: "React" },
    { id: "tauri", label: "Tauri" },
  ],
  components: [makeComponent({ name: "Desktop", kind: "desktop", technology: "Tauri" })],
});

const anilarim = makeProject({
  id: 2,
  name: "Anılarım",
  rootPath: "/home/dev/work/anilarim",
  ports: [3000, 4000],
  primaryUrl: { url: "http://localhost:3000", source: "devServer" },
  stack: [{ id: "nextjs", label: "Next.js" }],
  components: [
    makeComponent({ id: "web", name: "Web", kind: "web", technology: "Next.js" }),
    makeComponent({ id: "queue", name: "Jobs", kind: "worker", technology: "BullMQ" }),
  ],
});

describe("matchProject", () => {
  test("empty or whitespace-only queries match everything", () => {
    expect(matchProject(localdeck, [], "")).toBe(true);
    expect(matchProject(localdeck, [], "   ")).toBe(true);
    expect(matchProject(localdeck, [], "~")).toBe(true);
  });

  test("matches the project name case-insensitively", () => {
    expect(matchProject(localdeck, [], "localdeck")).toBe(true);
    expect(matchProject(localdeck, [], "LOCALD")).toBe(true);
    expect(matchProject(localdeck, [], "anilarim")).toBe(false);
  });

  test("matches the project's ports in every spelling", () => {
    for (const query of ["3000", ":3000", "localhost:3000", "http://localhost:3000", "127.0.0.1:3000"]) {
      expect(matchProject(anilarim, [], query)).toBe(true);
      expect(matchProject(localdeck, [], query)).toBe(false);
    }
  });

  test("port matching is a prefix match on owned ports only", () => {
    expect(matchProject(anilarim, [], "30")).toBe(true);
    expect(matchProject(anilarim, [], ":4000")).toBe(true);
    expect(matchProject(anilarim, [], "localhost:3001")).toBe(false);
    expect(matchProject(localdeck, [], ":9999")).toBe(false);
  });

  test("includes ports attributed through the snapshot's port rows", () => {
    const quiet = makeProject({ id: 3, name: "api", ports: [] });
    const ports = [
      makePort({ port: 8080, projectId: 3, ownership: "assigned" }),
      makePort({ port: 5432, projectId: null }),
    ];
    expect(matchProject(quiet, ports, ":8080")).toBe(true);
    expect(matchProject(quiet, ports, ":5432")).toBe(false);
  });

  test("matches root paths, including a ~-prefixed one", () => {
    expect(matchProject(localdeck, [], "/home/dev/projects")).toBe(true);
    expect(matchProject(localdeck, [], "~/projects/localdeck")).toBe(true);
    expect(matchProject(localdeck, [], "~/work/localdeck")).toBe(false);
  });

  test("matches stack labels and component name, kind and technology", () => {
    expect(matchProject(anilarim, [], "next")).toBe(true);
    expect(matchProject(anilarim, [], "bullmq")).toBe(true);
    expect(matchProject(anilarim, [], "worker")).toBe(true);
    expect(matchProject(anilarim, [], "jobs")).toBe(true);
    expect(matchProject(localdeck, [], "tauri")).toBe(true);
    expect(matchProject(localdeck, [], "desktop")).toBe(true);
    expect(matchProject(localdeck, [], "next")).toBe(false);
  });

  test("every token must match something", () => {
    expect(matchProject(anilarim, [], "next 3000")).toBe(true);
    expect(matchProject(anilarim, [], "next 9999")).toBe(false);
    expect(matchProject(anilarim, [], "web :4000 worker")).toBe(true);
  });

  test("does not special-case ignored projects (callers filter them first)", () => {
    expect(matchProject({ ...localdeck, ignored: true }, [], "localdeck")).toBe(true);
  });
});

describe("matchesStatusFilter", () => {
  const running = makeProject({ runtimeStatus: "running" });
  const partial = makeProject({ runtimeStatus: "partial" });
  const stopped = makeProject({ runtimeStatus: "stopped" });
  const stoppedWithAttention = makeProject({ runtimeStatus: "stopped", attention: [makeAttention()] });

  test("running includes partial; stopped is only stopped", () => {
    expect([running, partial, stopped].map((p) => matchesStatusFilter(p, "running"))).toEqual([true, true, false]);
    expect([running, partial, stopped].map((p) => matchesStatusFilter(p, "stopped"))).toEqual([false, false, true]);
  });

  test("attention is orthogonal to runtime status", () => {
    expect(matchesStatusFilter(stoppedWithAttention, "attention")).toBe(true);
    expect(matchesStatusFilter(stoppedWithAttention, "stopped")).toBe(true);
    expect(matchesStatusFilter(running, "attention")).toBe(false);
    expect(matchesStatusFilter(stopped, "all")).toBe(true);
  });
});
