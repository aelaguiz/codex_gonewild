import React from "react";
import { describe, it, expect, vi } from "vitest";
import { renderTui } from "./ui-test-helpers.js";
import TerminalChatInputThinking from "../src/components/chat/terminal-chat-input-thinking.js";

describe("TerminalChatInputThinking component", () => {
  it("renders with thinking status", async () => {
    const onInterrupt = vi.fn();
    const statusState = {
      description: "Testing component",
      startTime: Date.now(),
    };

    const { lastFrameStripped, cleanup } = renderTui(
      <TerminalChatInputThinking
        onInterrupt={onInterrupt}
        active={true}
        statusState={statusState}
      />,
    );

    const frame = lastFrameStripped();
    expect(frame).toContain("Thinking: Testing component");
    expect(frame).toContain("Press Esc twice to interrupt");

    cleanup();
  });
});
