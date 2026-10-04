/**
 * Keep object identity wherever `next` is deeply equal to `prev`.
 *
 * A fresh snapshot arrives every ~2 s as brand-new JSON objects. Reusing unchanged
 * sub-trees means components subscribed to one project or one array only re-render when
 * that part actually changed. Works on JSON values (plain objects, arrays, primitives).
 */
export function structuralShare<T>(prev: unknown, next: T): T {
  if (prev === next) return next;
  if (Array.isArray(next)) {
    if (!Array.isArray(prev)) return next;
    const merged: unknown[] = next.map((item, index) => structuralShare(prev[index], item));
    const same = merged.length === prev.length && merged.every((item, index) => item === prev[index]);
    return (same ? prev : merged) as T;
  }
  if (isPlainObject(next)) {
    if (!isPlainObject(prev)) return next;
    const nextKeys = Object.keys(next);
    const merged: Record<string, unknown> = {};
    let same = nextKeys.length === Object.keys(prev).length;
    for (const key of nextKeys) {
      merged[key] = structuralShare(prev[key], next[key]);
      if (merged[key] !== prev[key]) same = false;
    }
    return (same ? prev : merged) as T;
  }
  return next;
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
