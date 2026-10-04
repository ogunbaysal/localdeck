import { describe, expect, test } from "bun:test";
import { makeComponent, makePort, makeProject } from "@/test/fixtures";
import {
  buildPortRows,
  formatEndpoint,
  hostPort,
  matchesPortSearch,
  parsePortQuery,
} from "./port-model";

describe("parsePortQuery", () => {
  test("accepts a port in every spelling the UX promises", () => {
    for (const text of ["3000", " 3000 ", ":3000", "localhost:3000", "127.0.0.1:3000", "http://localhost:3000/"]) {
      expect(parsePortQuery(text)).toBe(3000);
    }
  });

  test("rejects words, mixed queries and out-of-range numbers", () => {
    for (const text of ["", "next", "3000 next", "localhost", "0", "65536", "123456"]) {
      expect(parsePortQuery(text)).toBeNull();
    }
  });
});

describe("buildPortRows", () => {
  const project = makeProject({ id: 1, components: [makeComponent({ id: "web", name: "Web" })] });

  test("only attributes assigned/shared endpoints; ambiguous and unassigned stay ownerless", () => {
    const rows = buildPortRows(
      [
        makePort({ port: 1, ownership: "assigned", projectId: 1, componentId: "web" }),
        makePort({ port: 2, ownership: "shared", projectId: 1 }),
        // A backend that (wrongly) sent ids for these must still never render an owner.
        makePort({ port: 3, ownership: "ambiguous", projectId: 1, componentId: "web" }),
        makePort({ port: 4, ownership: "unassigned", projectId: 1 }),
      ],
      [project],
    );
    expect(rows.map((row) => [row.project?.id, row.component?.name])).toEqual([
      [1, "Web"],
      [1, undefined],
      [undefined, undefined],
      [undefined, undefined],
    ]);
  });

  test("orders by port number", () => {
    const rows = buildPortRows([makePort({ port: 5432 }), makePort({ port: 80 })], []);
    expect(rows.map((row) => row.port.port)).toEqual([80, 5432]);
  });
});

describe("matchesPortSearch", () => {
  const [row] = buildPortRows(
    [
      makePort({
        port: 5173,
        ownership: "assigned",
        projectId: 1,
        componentId: "web",
        processes: [{ pid: 9, name: "vite", startedAt: null }],
      }),
    ],
    [makeProject({ id: 1, name: "LocalDeck", components: [makeComponent({ id: "web", name: "Frontend" })] })],
  );

  test("every token must match; port numbers match by prefix", () => {
    expect(matchesPortSearch(row, "")).toBe(true);
    expect(matchesPortSearch(row, "51")).toBe(true);
    expect(matchesPortSearch(row, ":5173 vite")).toBe(true);
    expect(matchesPortSearch(row, "localdeck frontend")).toBe(true);
    expect(matchesPortSearch(row, "5173 postgres")).toBe(false);
    expect(matchesPortSearch(row, "3000")).toBe(false);
  });
});

describe("display helpers", () => {
  test("endpoints bracket IPv6 and fold wildcards", () => {
    expect(formatEndpoint("127.0.0.1", 3000)).toBe("127.0.0.1:3000");
    expect(formatEndpoint("::1", 3000)).toBe("[::1]:3000");
    expect(formatEndpoint("0.0.0.0", 3000)).toBe("*:3000");
    expect(formatEndpoint("::", 3000)).toBe("*:3000");
  });

  test("hostPort reads host:port from a URL", () => {
    expect(hostPort("http://localhost:3000/app")).toBe("localhost:3000");
    expect(hostPort("not a url")).toBe("not a url");
  });
});
