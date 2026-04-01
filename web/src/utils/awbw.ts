const AWBW_BASE_URL = "https://awbw.amarriner.com";
const AWBW_FETCH_TIMEOUT_MS = 5000;

function decodeHtmlEntities(value: string): string {
  return value
    .replaceAll("&amp;", "&")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&#039;", "'");
}

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

export function parsePositiveIntegerParam(value: string): number | null {
  if (!/^\d+$/.test(value)) {
    return null;
  }

  const parsed = Number(value);
  return parsed > 0 && Number.isSafeInteger(parsed) ? parsed : null;
}

export async function fetchAwbwUsername(userId: number): Promise<Response> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => {
    controller.abort();
  }, AWBW_FETCH_TIMEOUT_MS);

  try {
    const response = await fetch(`${AWBW_BASE_URL}/profile.php?users_id=${userId}`, {
      signal: controller.signal,
    });
    if (!response.ok) {
      return Response.json({ userId, username: null }, { status: 502 });
    }

    const html = await response.text();
    return Response.json({
      userId,
      username: parseAwbwUsername(html),
    });
  } catch {
    return Response.json({ userId, username: null }, { status: 502 });
  } finally {
    clearTimeout(timeoutId);
  }
}

export async function fetchAwbwMap(mapId: number): Promise<Response> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => {
    controller.abort();
  }, AWBW_FETCH_TIMEOUT_MS);

  try {
    const response = await fetch(`${AWBW_BASE_URL}/api/map/map_info.php?maps_id=${mapId}`, {
      signal: controller.signal,
    });
    if (response.status === 404) {
      return new Response("Not Found", { status: 404 });
    }
    if (!response.ok) {
      return new Response("Bad Gateway", { status: 502 });
    }

    return new Response(await response.text(), {
      headers: {
        "Content-Type": response.headers.get("Content-Type") ?? "application/json",
      },
      status: response.status,
    });
  } catch {
    return new Response("Bad Gateway", { status: 502 });
  } finally {
    clearTimeout(timeoutId);
  }
}
