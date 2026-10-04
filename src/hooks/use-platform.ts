import { detectPlatform } from "@/lib/platform";
import { useRuntimeStore } from "@/stores/runtime-store";
import type { Platform } from "@/types";

/** The scanner's platform once a snapshot exists; a user-agent guess before that. */
export function usePlatform(): Platform {
  const reported = useRuntimeStore((state) => state.snapshot?.scanner.platform);
  return reported ?? detectPlatform(navigator.userAgent);
}
