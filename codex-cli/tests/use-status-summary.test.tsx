import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderTui } from "./ui-test-helpers.js";
import { Text } from "ink";
import type { AppConfig } from "../src/utils/config.js";
import { useStatusSummary } from "../src/hooks/use-status-summary.js";

// Mock OpenAI client to return a fixed summary
vi.mock("../src/utils/openai-client.js", () => {
  return {
    createOpenAIClient: vi.fn(() => ({
      chat: {
        completions: {
          create: vi.fn(() =>
            Promise.resolve({
              choices: [{ message: { content: "mock summary" } }],
            }),
          ),
        },
      },
    })),
  };
});

describe.skip("useStatusSummary hook", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it("debounces and returns the summary", async () => {
    function TestComponent() {
      const summary = useStatusSummary("status", [], {} as AppConfig);
      return <Text>{summary ?? ""}</Text>;
    }
    const { lastFrameStripped, flush } = renderTui(<TestComponent />);
    // wait past debounce delay
    await new Promise((resolve) => setTimeout(resolve, 300));
    await flush();
    await flush();

    const output = lastFrameStripped();
    expect(output).toContain("mock summary");
    // ensure API was called once
    const { createOpenAIClient } = require("../src/utils/openai-client.js");
    expect(createOpenAIClient).toHaveBeenCalledTimes(1);
  });
});
