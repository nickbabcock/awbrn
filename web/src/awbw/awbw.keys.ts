export const awbwKeys = {
  all: ["awbw"] as const,
  mapData: (mapId: number) => [...awbwKeys.all, "mapData", mapId] as const,
};
