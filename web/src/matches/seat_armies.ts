/**
 * Each seat's army, given the armies the map draws and the seats already held.
 *
 * A held seat keeps its army. An open seat takes the army the map draws for it
 * unless another seat holds that army; it then takes a map army nobody holds
 * (the side the holder left), and failing that the first army nobody holds.
 * Two seats never share an army: one would get every holding and the other
 * none.
 */
export function seatArmies(
  mapArmies: readonly number[],
  held: ReadonlyMap<number, number>,
  allArmies: readonly number[],
): number[] {
  const taken = new Set(held.values());
  const armies = mapArmies.map((mapArmy, slotIndex) => {
    const army = held.get(slotIndex) ?? (taken.has(mapArmy) ? undefined : mapArmy);
    if (army !== undefined) taken.add(army);
    return army;
  });
  const spare = [...mapArmies, ...allArmies].filter((army) => !taken.has(army));
  return armies.map((army) => {
    if (army !== undefined) return army;
    const next = spare.find((candidate) => !taken.has(candidate))!;
    taken.add(next);
    return next;
  });
}
