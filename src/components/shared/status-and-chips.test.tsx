import { describe, expect, mock, test } from "bun:test";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "@/test/render";
import { MetricsInline } from "./metrics-inline";
import { PortChips } from "./port-chips";
import { StatusBadge } from "./status-badge";

describe("StatusBadge", () => {
  test("labels each runtime status", () => {
    const { rerender } = renderWithProviders(<StatusBadge status="running" />);
    expect(screen.getByText("Running")).toBeInTheDocument();
    rerender(<StatusBadge status="partial" />);
    expect(screen.getByText("Partial")).toBeInTheDocument();
    rerender(<StatusBadge status="stopped" />);
    expect(screen.getByText("Stopped")).toBeInTheDocument();
  });

  test("a pending graceful stop reads Stopping…", () => {
    renderWithProviders(<StatusBadge status="running" stopPending />);
    expect(screen.getByText("Stopping…")).toBeInTheDocument();
    expect(screen.queryByText("Running")).not.toBeInTheDocument();
  });

  test("attention is an extra marker that also works on a stopped project", () => {
    const { rerender } = renderWithProviders(<StatusBadge status="stopped" />);
    expect(screen.queryByRole("img", { name: "Needs attention" })).not.toBeInTheDocument();
    rerender(<StatusBadge status="stopped" attention />);
    expect(screen.getByText("Stopped")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Needs attention" })).toBeInTheDocument();
  });
});

describe("PortChips", () => {
  test("renders nothing for no ports and collapses extras into +N", () => {
    const { container, rerender } = renderWithProviders(<PortChips ports={[]} />);
    expect(container).toBeEmptyDOMElement();
    rerender(<PortChips ports={[1420, 3000, 4000]} max={2} />);
    expect(screen.getByText(":1420")).toBeInTheDocument();
    expect(screen.getByText(":3000")).toBeInTheDocument();
    expect(screen.queryByText(":4000")).not.toBeInTheDocument();
    expect(screen.getByText("+1")).toBeInTheDocument();
  });

  test("chips become buttons only with a click handler", async () => {
    const onPortClick = mock((_port: number) => {});
    const { user, rerender } = renderWithProviders(<PortChips ports={[3000]} />);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    rerender(<PortChips ports={[3000]} onPortClick={onPortClick} />);
    await user.click(screen.getByRole("button", { name: ":3000" }));
    expect(onPortClick).toHaveBeenCalledWith(3000);
  });
});

describe("MetricsInline", () => {
  test("shows cpu, memory and process count", () => {
    renderWithProviders(<MetricsInline metrics={{ cpuPercent: 4.2, memoryBytes: 286 * 1024 * 1024, processCount: 4 }} />);
    expect(screen.getByText("4.2% CPU")).toBeInTheDocument();
    expect(screen.getByText("286 MB")).toBeInTheDocument();
    expect(screen.getByText("4 processes")).toBeInTheDocument();
  });
});
