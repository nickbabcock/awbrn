/** What a failed AWBW map import tells the player. */
export function importFailureMessage(error: unknown): string {
  const message = error instanceof Error ? error.message : "";
  if (/not found|404/i.test(message)) return "AWBW has no map with that id";
  if (/signed in|unauthorized|401/i.test(message)) return "Sign in to import a map";
  if (/rate|429|too many/i.test(message)) return "Too many imports, try again in a minute";
  return "The map could not be imported";
}
