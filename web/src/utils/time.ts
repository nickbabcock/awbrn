export function formatRelativeTime(iso: string, relativeToMs: number): string {
  const deltaMs = relativeToMs - Date.parse(iso);
  const deltaMinutes = Math.round(deltaMs / 60_000);
  const formatter = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

  if (Math.abs(deltaMinutes) < 60) return formatter.format(-deltaMinutes, "minute");

  const deltaHours = Math.round(deltaMinutes / 60);
  if (Math.abs(deltaHours) < 24) return formatter.format(-deltaHours, "hour");

  return formatter.format(-Math.round(deltaHours / 24), "day");
}

/**
 * A short duration, for a countdown or an elapsed wait.
 *
 * The output keeps two units at most, so it stays the same width as it
 * counts down: "3d 4h", "14h 02m", "28m", "45s".
 */
export function formatCompactDuration(durationMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes.toString().padStart(2, "0")}m`;
  if (minutes > 0) return `${minutes}m`;
  return `${seconds}s`;
}
