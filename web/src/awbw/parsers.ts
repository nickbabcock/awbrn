import { awbwGamePagePlayerSchema, type AwbwGameSnapshot } from "./schemas";

export function parseAwbwUsername(html: string): string | null {
  const usernameIndex = html.indexOf("Username:");
  if (usernameIndex < 0) {
    return null;
  }

  const startMarker = html.indexOf("<i>", usernameIndex);
  if (startMarker < 0) {
    return null;
  }

  const start = startMarker + 3;
  const end = html.indexOf("</i>", start);
  if (end < 0) {
    return null;
  }

  return decodeHtmlEntities(html.slice(start, end).trim());
}

/**
 * The armies, map and turn of an AWBW game page, or null when the page has no
 * game.
 *
 * The page embeds its state as script assignments, such as
 * `let playersInfo = {...};`. A username can hold any text, so the object is
 * read to its closing brace, not to the next semicolon.
 */
export function parseAwbwGame(html: string, gameId: number): AwbwGameSnapshot | null {
  const playersInfo = readScriptObject(html, "playersInfo");
  if (playersInfo === null || typeof playersInfo !== "object") {
    return null;
  }

  const armies = [];
  for (const value of Object.values(playersInfo)) {
    const parsed = awbwGamePagePlayerSchema.safeParse(value);
    if (!parsed.success) {
      return null;
    }
    const player = parsed.data;
    armies.push({
      playerId: player.players_id,
      username: decodeHtmlEntities(player.users_username),
      factionCode: player.countries_code,
      team: player.players_team,
      order: player.players_order,
      eliminated: player.players_eliminated === "Y",
      coId: player.players_co_id,
      coPower: player.players_co_power,
      powerOn: ({ N: "none", Y: "cop", S: "scop" } as const)[player.players_co_power_on],
      copBaseCost: player.co_max_power,
      copCost: player.players_co_max_power,
      scopBaseCost: player.co_max_spower,
      scopCost: player.players_co_max_spower,
      funds:
        typeof player.players_funds === "number"
          ? player.players_funds
          : /^\d+$/.test(player.players_funds)
            ? Number(player.players_funds)
            : null,
    });
  }
  armies.sort((left, right) => left.order - right.order);

  const activePlayerId = /\blet currentTurn = (\d+);/.exec(html)?.[1];
  const mapId = /prevmaps\.php\?maps_id=(\d+)/.exec(html)?.[1];
  return {
    gameId,
    mapId: mapId === undefined ? null : Number(mapId),
    activePlayerId: activePlayerId === undefined ? null : Number(activePlayerId),
    armies,
  };
}

/** The JSON object of the script assignment `let name = {...};`. */
function readScriptObject(html: string, name: string): unknown {
  const marker = `let ${name} = `;
  const start = html.indexOf(marker);
  if (start < 0 || html[start + marker.length] !== "{") {
    return null;
  }

  const begin = start + marker.length;
  let depth = 0;
  let inString = false;
  for (let index = begin; index < html.length; index += 1) {
    const char = html[index];
    if (inString) {
      if (char === "\\") {
        index += 1;
      } else if (char === '"') {
        inString = false;
      }
    } else if (char === '"') {
      inString = true;
    } else if (char === "{") {
      depth += 1;
    } else if (char === "}") {
      depth -= 1;
      if (depth === 0) {
        try {
          return JSON.parse(html.slice(begin, index + 1)) as unknown;
        } catch {
          return null;
        }
      }
    }
  }
  return null;
}

/**
 * The game ID in an AWBW game link, such as
 * `https://awbw.amarriner.com/game.php?games_id=1710706`, or in a bare
 * number. Null when the text has no game ID.
 */
export function parseAwbwGameId(text: string): number | null {
  const trimmed = text.trim();
  const id = /^\d+$/.test(trimmed)
    ? trimmed
    : /^(?:https?:\/\/)?awbw\.amarriner\.com\/[\w.]+\.php\?(?:[^#]*&)?games_id=(\d+)/.exec(
        trimmed,
      )?.[1];
  return id === undefined ? null : parsePositiveIntegerParam(id);
}

export function parsePositiveIntegerParam(value: string): number | null {
  if (!/^\d+$/.test(value)) {
    return null;
  }

  const parsed = Number(value);
  return parsed > 0 && Number.isSafeInteger(parsed) ? parsed : null;
}

function decodeHtmlEntities(value: string): string {
  return value
    .replaceAll("&amp;", "&")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&#039;", "'");
}
