/**
 * Whether a ban is in force.
 *
 * A ban with no expiry does not expire. A ban that has passed its expiry is
 * over without anything having to clear the column, so the record of it
 * stays readable.
 */
export function isBanned(
  banned: boolean | null | undefined,
  banExpires: Date | null | undefined,
  now: Date = new Date(),
): boolean {
  if (!banned) return false;
  if (banExpires === null || banExpires === undefined) return true;
  return banExpires.getTime() > now.getTime();
}
