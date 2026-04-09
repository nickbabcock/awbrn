export function awbwMapAssetPath(mapId: number): string {
  return `/api/awbw/map/${mapId}.json`;
}

export function awbwSmallMapAssetPath(mapId: number): string {
  return `/api/awbw/smallmap/${mapId}.png`;
}
