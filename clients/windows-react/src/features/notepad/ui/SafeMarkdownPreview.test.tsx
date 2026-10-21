import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SafeMarkdownPreview } from "./SafeMarkdownPreview";

describe("SafeMarkdownPreview", () => {
  it("renders common Markdown structure as text without interpreting HTML", () => {
    render(<SafeMarkdownPreview markdown={"# 标题\n\n- A\n- B\n\n<script>alert(1)</script>"} />);
    expect(screen.getByRole("heading", { name: "标题" })).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByText("<script>alert(1)</script>")).toBeInTheDocument();
    expect(document.querySelector("script")).toBeNull();
  });

  it("renders only bounded supported image data URLs", () => {
    render(<SafeMarkdownPreview markdown="![screen](<data:image/png;base64,iVBORw==>)" />);
    expect(screen.getByRole("img", { name: "screen" })).toHaveAttribute("src", "data:image/png;base64,iVBORw==");
  });
});
