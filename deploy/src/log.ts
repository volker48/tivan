/**
 * Structured one-line log for Workers Logs. Callers pass codes and counts only, never tokens,
 * cookies or roster data.
 */
export function logEvent(event: string, fields: Record<string, string | number> = {}): void {
  console.error(JSON.stringify({ event, ...fields }));
}

export function errorName(error: unknown): string {
  return error instanceof Error ? error.name : "unknown";
}
