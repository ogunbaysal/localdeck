import { useRuntimeStore } from "@/stores/runtime-store";
import { useUiStore } from "@/stores/ui-store";

/** Put both Zustand stores back to their initial state. */
export function resetStores(): void {
  useRuntimeStore.setState(useRuntimeStore.getInitialState(), true);
  useUiStore.setState(useUiStore.getInitialState(), true);
}
