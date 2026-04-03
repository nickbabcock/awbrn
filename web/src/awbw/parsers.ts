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

function decodeHtmlEntities(value: string): string {
  return value
    .replaceAll("&amp;", "&")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&#039;", "'");
}
