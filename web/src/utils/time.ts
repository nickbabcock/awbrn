export function formatRelativeTime(iso: string, relativeToMs: number): string {
  const deltaMs = relativeToMs - Date.parse(iso);
  const deltaMinutes = Math.round(deltaMs / 60_000);
  const formatter = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

  if (Math.abs(deltaMinutes) < 60) return formatter.format(-deltaMinutes, "minute");

  const deltaHours = Math.round(deltaMinutes / 60);
  if (Math.abs(deltaHours) < 24) return formatter.format(-deltaHours, "hour");

  return formatter.format(-Math.round(deltaHours / 24), "day");
}
