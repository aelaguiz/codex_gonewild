/* eslint-disable @typescript-eslint/no-explicit-any */
import type { AppConfig } from "../utils/config.js";

import { log, isLoggingEnabled } from "../utils/logger/log.js";
import { createOpenAIClient } from "../utils/openai-client.js";
import { useEffect, useRef, useState } from "react";

/**
 * Serialize a list of response items into a simple text history.
 */
function serializeHistory(history: Array<any>): string {
  return history
    .map((item: any) => {
      const role = item.role;
      const text = item.content
        .map((c: any) => (c.type === "output_text" ? c.text : ""))
        .join("");
      return `${role}: ${text.trim()}`;
    })
    .join("\n");
}

/**
 * Hook that generates a one-line summary of the current agent status
 * by calling a fast summarization model after a debounce.
 *
 * @param status Current status label (e.g. "Calling local shell")
 * @param history Full chat history items
 * @param config Application configuration for OpenAI client
 * @returns A one-line summary or undefined while loading or on error
 */
export function useStatusSummary(
  status: string,
  history: Array<any>,
  config: AppConfig,
): string | undefined {
  const [summary, setSummary] = useState<string>();
  const timeoutRef = useRef<NodeJS.Timeout>();
  const abortCtrlRef = useRef<AbortController>();

  useEffect(() => {
    // Clear any pending timers or inflight requests
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }
    if (abortCtrlRef.current) {
      abortCtrlRef.current.abort();
    }
    setSummary(undefined);

    // Normalize empty status so we always generate an "Idle" summary instead of dropping away
    const normalizedStatus = status.trim() || "Idle";

    // Debounce to avoid rapid-fire API calls
    timeoutRef.current = setTimeout(() => {
      const controller = new AbortController();
      abortCtrlRef.current = controller;
      (async () => {
        try {
          const client = createOpenAIClient(config);
          const messages = [
            {
              role: "system",
              content:
                "You are a CLI assistant. Summarize the agent status below in one concise sentence.",
            },
            {
              role: "user",
              content: `Status: ${normalizedStatus}\nHistory:\n${serializeHistory(
                history,
              )}`,
            },
          ] as any;
          if (isLoggingEnabled()) {
            log(
              `[status-summary] prompt messages:\n${JSON.stringify(
                messages,
                null,
                2,
              )}`,
            );
          }
          const resp = await client.chat.completions.create(
            { model: "gpt-4o-mini", messages, max_tokens: 30 },
            { signal: controller.signal },
          );
          const raw = resp.choices?.[0]?.message.content ?? "";
          const text = raw.trim();
          if (isLoggingEnabled()) {
            log(`[status-summary] response content: ${text}`);
          }
          setSummary(text);
        } catch {
          // ignore errors and leave summary undefined
        }
      })();
    }, 300);

    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
      if (abortCtrlRef.current) {
        abortCtrlRef.current.abort();
      }
    };
  }, [status, history, config]);

  return summary;
}
