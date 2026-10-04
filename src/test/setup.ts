import { GlobalRegistrator } from "@happy-dom/global-registrator";
import { afterEach, expect, mock } from "bun:test";
import { backend } from "./backend";
import { resetStores } from "./reset";

// The DOM must exist before Testing Library is first evaluated (it reads `document` at
// import time; jest-dom's matchers import it too), and static imports would be evaluated
// before this call: import both dynamically, after registering.
GlobalRegistrator.register({ url: "http://localhost/", width: 1024, height: 768 });
// `default` is the CJS interop copy of the same matchers; expect.extend wants functions only.
const { default: _cjsDefault, ...matchers } = await import("@testing-library/jest-dom/matchers");
const { cleanup } = await import("@testing-library/react");

// The real `@/lib/tauri` wrappers run against the fake backend (see ./backend.ts).
mock.module("@tauri-apps/api/core", () => ({ invoke: backend.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: backend.listen }));

expect.extend(matchers);

afterEach(() => {
  cleanup();
  backend.reset();
  resetStores();
  document.documentElement.className = "";
  delete document.documentElement.dataset.platform;
});
