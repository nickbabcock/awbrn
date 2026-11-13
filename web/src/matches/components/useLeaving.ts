import { useEffect, useState } from "react";

/**
 * How long a closed sheet stays mounted: past its 90ms stepped exit, with room
 * for the frame the exit starts on. It is invisible by then, so a longer hold
 * costs nothing.
 */
const LEAVING_HOLD_MS = 200;

/**
 * Holds the last value a sheet showed while the sheet leaves.
 *
 * A bottom sheet plays its exit only while it is still mounted with `isOpen`
 * false. A sheet whose content comes from state that clears on dismissal (the
 * engine's build site, an open flag) would otherwise unmount in the same
 * commit and cut. While `value` is set this returns it, open; once it clears,
 * it returns the last one, closed, for the exit, and then null.
 *
 *   const sheet = useLeaving(production);
 *   {sheet.value ? <Sheet isOpen={sheet.isOpen} site={sheet.value.site} /> : null}
 */
export function useLeaving<T>(value: T | null): { value: T | null; isOpen: boolean } {
  const [held, setHeld] = useState<T | null>(value);
  if (value !== null && value !== held) setHeld(value);

  useEffect(() => {
    if (value !== null || held === null) return;
    const timer = setTimeout(() => setHeld(null), LEAVING_HOLD_MS);
    return () => clearTimeout(timer);
  }, [value, held]);

  return value === null ? { value: held, isOpen: false } : { value, isOpen: true };
}
