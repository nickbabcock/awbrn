import { describe, expect, it } from "vitest";
import {
  base64UrlDecode,
  base64UrlEncode,
  encryptPushPayload,
  vapidAuthorization,
} from "./web_push.ts";

/** A browser's half of a subscription: the key pair and the shared secret. */
async function createUserAgentKeys() {
  const pair = (await crypto.subtle.generateKey({ name: "ECDH", namedCurve: "P-256" }, true, [
    "deriveBits",
  ])) as CryptoKeyPair;
  const publicKey = new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey));
  const auth = crypto.getRandomValues(new Uint8Array(16));
  return {
    privateKey: pair.privateKey,
    subscription: { p256dh: base64UrlEncode(publicKey), auth: base64UrlEncode(auth) },
    publicKey,
    auth,
  };
}

function concat(...parts: Uint8Array[]): Uint8Array {
  const joined = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    joined.set(part, offset);
    offset += part.length;
  }
  return joined;
}

async function hkdf(
  salt: Uint8Array,
  ikm: Uint8Array,
  info: Uint8Array,
  length: number,
): Promise<Uint8Array> {
  const extract = await crypto.subtle.importKey(
    "raw",
    salt as BufferSource,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const prk = new Uint8Array(await crypto.subtle.sign("HMAC", extract, ikm as BufferSource));
  const expand = await crypto.subtle.importKey(
    "raw",
    prk as BufferSource,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const block = new Uint8Array(
    await crypto.subtle.sign("HMAC", expand, concat(info, Uint8Array.of(1)) as BufferSource),
  );
  return block.slice(0, length);
}

/**
 * Read a message back the way a browser does.
 *
 * This is the other side of RFC 8291 written out independently, so a mistake
 * in the derivation shows up as a payload that will not decrypt rather than as
 * a body that merely looks well formed.
 */
async function decryptAsUserAgent(
  body: Uint8Array,
  userAgentPublicKey: Uint8Array,
  userAgentPrivateKey: CryptoKey,
  authSecret: Uint8Array,
): Promise<string> {
  const salt = body.slice(0, 16);
  const keyLength = body[20]!;
  const senderPublicKey = body.slice(21, 21 + keyLength);
  const ciphertext = body.slice(21 + keyLength);

  const sender = await crypto.subtle.importKey(
    "raw",
    senderPublicKey as BufferSource,
    { name: "ECDH", namedCurve: "P-256" },
    false,
    [],
  );
  const shared = new Uint8Array(
    await crypto.subtle.deriveBits({ name: "ECDH", public: sender }, userAgentPrivateKey, 256),
  );

  const keyInfo = concat(
    new TextEncoder().encode("WebPush: info"),
    Uint8Array.of(0),
    userAgentPublicKey,
    senderPublicKey,
  );
  const ikm = await hkdf(authSecret, shared, keyInfo, 32);
  const cek = await hkdf(
    salt,
    ikm,
    concat(new TextEncoder().encode("Content-Encoding: aes128gcm"), Uint8Array.of(0)),
    16,
  );
  const nonce = await hkdf(
    salt,
    ikm,
    concat(new TextEncoder().encode("Content-Encoding: nonce"), Uint8Array.of(0)),
    12,
  );

  const aesKey = await crypto.subtle.importKey(
    "raw",
    cek as BufferSource,
    { name: "AES-GCM" },
    false,
    ["decrypt"],
  );
  const plaintext = new Uint8Array(
    await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: nonce as BufferSource, tagLength: 128 },
      aesKey,
      ciphertext as BufferSource,
    ),
  );
  // The last byte is the delimiter that marks the final record.
  expect(plaintext[plaintext.length - 1]).toBe(2);
  return new TextDecoder().decode(plaintext.slice(0, -1));
}

describe("encryptPushPayload", () => {
  it("writes a body the subscribed browser can read back", async () => {
    const agent = await createUserAgentKeys();
    const message = JSON.stringify({ type: "turn", matchId: "abc123" });

    const body = await encryptPushPayload(agent.subscription, new TextEncoder().encode(message));

    await expect(
      decryptAsUserAgent(body, agent.publicKey, agent.privateKey, agent.auth),
    ).resolves.toBe(message);
  });

  it("lays out the header the content encoding calls for", async () => {
    const agent = await createUserAgentKeys();
    const salt = crypto.getRandomValues(new Uint8Array(16));

    const body = await encryptPushPayload(agent.subscription, Uint8Array.of(1, 2, 3), { salt });

    expect(Array.from(body.slice(0, 16))).toEqual(Array.from(salt));
    expect(new DataView(body.buffer, body.byteOffset).getUint32(16, false)).toBe(4096);
    expect(body[20]).toBe(65);
    // The throwaway key is a point on the curve, not the browser's own.
    expect(body[21]).toBe(0x04);
    expect(Array.from(body.slice(21, 86))).not.toEqual(Array.from(agent.publicKey));
    // Three bytes of payload, the delimiter, and the tag.
    expect(body.length).toBe(86 + 3 + 1 + 16);
  });

  it("cannot be read with another browser's keys", async () => {
    const agent = await createUserAgentKeys();
    const other = await createUserAgentKeys();

    const body = await encryptPushPayload(agent.subscription, new TextEncoder().encode("secret"));

    await expect(
      decryptAsUserAgent(body, other.publicKey, other.privateKey, other.auth),
    ).rejects.toThrow();
  });

  it("refuses a payload too large for one record", async () => {
    const agent = await createUserAgentKeys();

    await expect(encryptPushPayload(agent.subscription, new Uint8Array(4096))).rejects.toThrow(
      /too large/,
    );
  });
});

describe("vapidAuthorization", () => {
  it("signs a token the push service can check against the sent key", async () => {
    const pair = (await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, [
      "sign",
      "verify",
    ])) as CryptoKeyPair;
    const publicKey = base64UrlEncode(
      new Uint8Array(await crypto.subtle.exportKey("raw", pair.publicKey)),
    );
    const jwk = await crypto.subtle.exportKey("jwk", pair.privateKey);

    const header = await vapidAuthorization("https://fcm.googleapis.com/fcm/send/abc", {
      publicKey,
      privateKey: jwk.d!,
      subject: "mailto:nobody@example.com",
    });

    const match = /^vapid t=([\w-]+\.[\w-]+)\.([\w-]+), k=([\w-]+)$/.exec(header);
    expect(match).not.toBeNull();
    const [, signingInput, signature, sentKey] = match!;
    expect(sentKey).toBe(publicKey);

    const verified = await crypto.subtle.verify(
      { name: "ECDSA", hash: "SHA-256" },
      pair.publicKey,
      base64UrlDecode(signature!) as BufferSource,
      new TextEncoder().encode(signingInput!) as BufferSource,
    );
    expect(verified).toBe(true);

    const claims = JSON.parse(
      new TextDecoder().decode(base64UrlDecode(signingInput!.split(".")[1]!)),
    ) as { aud: string; exp: number; sub: string };
    // The signature names the push service, so it cannot be replayed at another.
    expect(claims.aud).toBe("https://fcm.googleapis.com");
    expect(claims.sub).toBe("mailto:nobody@example.com");
    expect(claims.exp).toBeGreaterThan(Math.floor(Date.now() / 1000));
    // Push services refuse anything more than a day out.
    expect(claims.exp).toBeLessThan(Math.floor(Date.now() / 1000) + 24 * 60 * 60);
  });
});
