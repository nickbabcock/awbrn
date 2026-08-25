import type { z } from "zod";

/** Decode and validate a JSON cursor. */
export function decodeCursor<T>(cursor: string | undefined, schema: z.ZodType<T>): T | null {
  if (!cursor) return null;

  try {
    const result = schema.safeParse(JSON.parse(cursor));
    return result.success ? result.data : null;
  } catch {
    return null;
  }
}
