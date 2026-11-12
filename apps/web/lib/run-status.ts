/**
 * The vocabulary of a run, as the screens show it.
 *
 * The wire spells a status in snake case (`waiting_input`); the screens spell it in words
 * ("Waiting for your answer"). The eight statuses are the eight the domain knows, and every one
 * of them is written out: no state is ever carried by a colour alone (REQ-012).
 */

export const RUN_STATUSES = [
  "queued",
  "leased",
  "running",
  "waiting_input",
  "waiting_takeover",
  "completed",
  "failed",
  "cancelled",
] as const;

export type RunStatus = (typeof RUN_STATUSES)[number];

/** Whether a value is one of the eight statuses. */
export function isRunStatus(value: string): value is RunStatus {
  return (RUN_STATUSES as readonly string[]).includes(value);
}

/** The catalogue key of a status, or `null` when the wire carries one we do not know. */
export function runStatusKey(status: string): string | null {
  return isRunStatus(status) ? `run.status.${status}` : null;
}

/** The catalogue key of a trigger, or `null` when the wire carries one we do not know. */
export function runTriggerKey(trigger: string): string | null {
  const known = [
    "user",
    "routine",
    "webhook",
    "bot_message",
    "tickets",
    "created",
    "helper_result",
    "retry",
  ];
  return known.includes(trigger) ? `run.trigger.${trigger}` : null;
}

/** Whether a failed run may be retried: the only status that may (REQ-012). */
export function isRetryable(status: string): boolean {
  return status === "failed";
}
