import { FolderPlus } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router";
import { Button } from "@/components/ui/button";
import { runAction } from "@/lib/actions";
import { pickAndAddProject } from "@/lib/tauri";

/**
 * Opens the native folder picker (owned by the backend), remembers the chosen folder as a
 * project and opens its detail page. Cancelling the picker does nothing.
 */
export function AddProjectButton({ variant = "outline" }: { variant?: "default" | "outline" | "secondary" }) {
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);

  const add = async () => {
    setBusy(true);
    try {
      const id = await runAction("Add project", pickAndAddProject, { success: false });
      if (typeof id === "number") void navigate(`/projects/${id}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Button type="button" variant={variant} size="sm" disabled={busy} onClick={() => void add()}>
      <FolderPlus aria-hidden />
      Add project
    </Button>
  );
}
