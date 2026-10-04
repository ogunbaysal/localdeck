import { describe, expect, test } from "bun:test";
import { structuralShare } from "./structural-share";

describe("structuralShare", () => {
  test("returns the previous reference when nothing changed", () => {
    const prev = { a: [1, 2, { b: "x" }], c: null };
    expect(structuralShare(prev, { a: [1, 2, { b: "x" }], c: null })).toBe(prev);
  });

  test("keeps unchanged siblings while replacing the changed branch", () => {
    const prev = { stable: { n: 1 }, list: [{ id: 1, v: 1 }, { id: 2, v: 2 }] };
    const next = structuralShare(prev, { stable: { n: 1 }, list: [{ id: 1, v: 1 }, { id: 2, v: 3 }] });
    expect(next).not.toBe(prev);
    expect(next.stable).toBe(prev.stable);
    expect(next.list).not.toBe(prev.list);
    expect(next.list[0]).toBe(prev.list[0]);
    expect(next.list[1]).toEqual({ id: 2, v: 3 });
  });

  test("detects length changes in arrays and key changes in objects", () => {
    const prev = { list: [1, 2], o: { a: 1 } };
    const next = structuralShare(prev, { list: [1], o: { a: 1, b: 2 } });
    expect(next.list).toEqual([1]);
    expect(next.o).toEqual({ a: 1, b: 2 });
  });

  test("never mixes up types", () => {
    expect(structuralShare([1], { a: 1 })).toEqual({ a: 1 });
    expect(structuralShare({ a: 1 }, [1])).toEqual([1]);
    expect(structuralShare(null, { a: 1 })).toEqual({ a: 1 });
  });
});
