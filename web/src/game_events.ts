export interface NewDayEvent {
  type: "NewDay";
  day: number;
}

export interface UnitMovedEvent {
  type: "UnitMoved";
  unitId: number;
  fromX: number;
  fromY: number;
  toX: number;
  toY: number;
}

export interface UnitBuiltEvent {
  type: "UnitBuilt";
  unitId: number;
  unitType: string;
  x: number;
  y: number;
  playerId: number;
}

export interface TileSelectedEvent {
  type: "TileSelected";
  x: number;
  y: number;
  terrainType: string;
}

export type GameEvent =
  | NewDayEvent
  | UnitMovedEvent
  | UnitBuiltEvent
  | TileSelectedEvent;
