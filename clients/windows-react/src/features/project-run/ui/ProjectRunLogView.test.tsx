import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ProjectRunLogView } from "./ProjectRunLogView";

describe("ProjectRunLogView", () => {
  afterEach(cleanup);

  it("renders selectable numbered output with an accessible log region", () => {
    const { container } = renderLog({ content: "first\nsecond\n" });

    expect(screen.getByRole("region", { name: "Web 开发服务器运行日志" })).toBeInTheDocument();
    expect(screen.getByLabelText("Web 开发服务器运行日志内容").textContent).toBe("first\nsecond\n");
    expect(container.querySelector(".project-run-log__gutter")?.textContent).toBe("1\n2\n3");
    expect(screen.getByLabelText("Web 开发服务器运行日志内容")).toHaveAttribute("tabindex", "0");
  });

  it("follows new output while running and synchronizes the line-number gutter", () => {
    const { container, rerender } = renderLog({ content: "first" });
    const viewport = screen.getByLabelText("Web 开发服务器运行日志内容");
    const gutter = container.querySelector(".project-run-log__gutter") as HTMLElement;
    Object.defineProperties(viewport, {
      scrollHeight: { configurable: true, value: 420 },
      clientHeight: { configurable: true, value: 120 },
    });

    rerender(<LogFixture content="first\nsecond" followsTail />);

    expect(viewport.scrollTop).toBe(300);
    expect(gutter.scrollTop).toBe(300);
    viewport.scrollTop = 90;
    fireEvent.scroll(viewport);
    expect(gutter.scrollTop).toBe(90);
  });

  it("preserves the viewport when a stopped instance receives its final state", () => {
    const { rerender } = renderLog({ content: "first", followsTail: false });
    const viewport = screen.getByLabelText("Web 开发服务器运行日志内容");
    viewport.scrollTop = 64;

    rerender(<LogFixture content="first\nlast" followsTail={false} />);

    expect(viewport.scrollTop).toBe(64);
    expect(screen.getByText("已结束")).toBeInTheDocument();
  });

  it("states when the bounded native log projection was truncated", () => {
    render(<LogFixture content="tail" followsTail={false} truncated />);
    expect(screen.getByText("日志已达到 512 KiB 上限，只保留最近输出。")).toBeInTheDocument();
  });
});

function renderLog({ content, followsTail = true }: { content: string; followsTail?: boolean }) {
  return render(<LogFixture content={content} followsTail={followsTail} />);
}

function LogFixture({
  content,
  followsTail,
  truncated = false,
}: {
  content: string;
  followsTail: boolean;
  truncated?: boolean;
}) {
  return (
    <ProjectRunLogView
      content={content}
      followsTail={followsTail}
      label="Web 开发服务器运行日志"
      status={followsTail ? "实时更新 · 跟随末尾" : "已结束"}
      truncated={truncated}
    />
  );
}
