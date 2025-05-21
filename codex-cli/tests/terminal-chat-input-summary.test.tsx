import React from "react";
import type { ComponentProps } from "react";
import { describe, it, expect } from "vitest";
import { renderTui } from "./ui-test-helpers.js";
import TerminalChatInput from "../src/components/chat/terminal-chat-input.js";

describe("TerminalChatInput summary line", () => {
  it("renders the status summary above the prompt", () => {
    const props: ComponentProps<typeof TerminalChatInput> = {
      isNew: false,
      loading: false,
      submitInput: () => {},
      confirmationPrompt: null,
      explanation: undefined,
      submitConfirmation: () => {},
      setLastResponseId: () => {},
      setItems: () => {},
      contextLeftPercent: 100,
      openOverlay: () => {},
      openDiffOverlay: () => {},
      openModelOverlay: () => {},
      openApprovalOverlay: () => {},
      openHelpOverlay: () => {},
      openSessionsOverlay: () => {},
      onCompact: () => {},
      interruptAgent: () => {},
      active: true,
      totalThinkingSeconds: 0,
      statusState: { description: "", startTime: undefined },
      statusSummary: "One-line summary",
    };
    const { lastFrameStripped } = renderTui(<TerminalChatInput {...props} />);
    const frame = lastFrameStripped();
    expect(frame).toContain("⚡ One-line summary");
  });
});
