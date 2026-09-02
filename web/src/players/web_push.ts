/**
 * Web push delivery, encrypted the way the push services require.
 *
 * A push service carries a payload it cannot read, so the message is encrypted
 * to the browser's own key before it is handed over (RFC 8291), and the request
 * is signed so the service knows which application server sent it (RFC 8292).
 * Both are built here on the WebCrypto the workers runtime already has, which
 * is why this file has no dependency to keep current.
 */

/** The keys a browser hands out when a player allows notifications. */
export interface PushSubscription {
  /** Where the push service takes the message. */
  endpoint: string;
  /** The browser's public key, 65 bytes, base64url. */
  p256dh: string;
  /** The shared secret the browser generated, 16 bytes, base64url. */
  auth: string;
}

/** The application server's identity, as the push services check it. */
export interface VapidKeys {
  /** 65 bytes, base64url. The same value the browser subscribes with. */
  publicKey: string;
  /** The private scalar, 32 bytes, base64url. */
  privateKey: string;
  /** Who to contact about this sender: a `mailto:` or `https:` URL. */
  subject: string;
}

export interface VapidEnvironment {
  VAPID_PUBLIC_KEY?: string;
  VAPID_PRIVATE_KEY?: string;
  VAPID_SUBJECT?: string;
}

export function readVapidKeys(environment: VapidEnvironment): VapidKeys | null {
  const publicKey = environment.VAPID_PUBLIC_KEY;
  const privateKey = environment.VAPID_PRIVATE_KEY;
  const subject = environment.VAPID_SUBJECT;
  if (!publicKey || !privateKey || !subject) {
    return null;
  }
  return { publicKey, privateKey, subject };
}

/** How long a push service should hold a message for a browser that is away. */
export const DEFAULT_PUSH_TTL_SECONDS = 24 * 60 * 60;

/**
 * How long a signature stays good.
 *
 * The push services refuse anything more than 24 hours out, so this sits well
 * inside that and is still long enough that one signature covers a burst.
 */
const VAPID_EXPIRY_SECONDS = 12 * 60 * 60;

/**
 * The record size written into the header.
 *
 * One record carries the whole payload, so this only has to be larger than the
 * largest message plus its padding and tag, and the push services cap what
 * they accept at 4096 bytes anyway.
 */
const RECORD_SIZE = 4096;

/** How much of a record the delimiter and the tag take, leaving the rest. */
const RECORD_OVERHEAD = 1 + 16;

const P256_PUBLIC_KEY_BYTES = 65;
const SALT_BYTES = 16;

/** The salt, the record size, the key length, and the key itself. */
const HEADER_BYTES = SALT_BYTES + 4 + 1 + P256_PUBLIC_KEY_BYTES;

export function base64UrlDecode(value: string): Uint8Array {
  const padded = value.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(padded.padEnd(Math.ceil(padded.length / 4) * 4, "="));
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

export function base64UrlEncode(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function concatBytes(...parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const joined = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    joined.set(part, offset);
    offset += part.length;
  }
  return joined;
}

/**
 * One block of HKDF, which is all this needs.
 *
 * Every value derived here is 32 bytes or shorter, so the expand step never
 * runs past its first block and the counter is always a single `0x01`.
 */
async function hkdf(
  salt: Uint8Array,
  inputKeyMaterial: Uint8Array,
  info: Uint8Array,
  length: number,
): Promise<Uint8Array> {
  const extractKey = await crypto.subtle.importKey(
    "raw",
    salt as BufferSource,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const pseudoRandomKey = new Uint8Array(
    await crypto.subtle.sign("HMAC", extractKey, inputKeyMaterial as BufferSource),
  );
  const expandKey = await crypto.subtle.importKey(
    "raw",
    pseudoRandomKey as BufferSource,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const block = new Uint8Array(
    await crypto.subtle.sign(
      "HMAC",
      expandKey,
      concatBytes(info, Uint8Array.of(1)) as BufferSource,
    ),
  );
  return block.slice(0, length);
}

function utf8(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

/** Split an uncompressed P-256 point into the coordinates a JWK names. */
function pointToJwkCoordinates(publicKey: Uint8Array): { x: string; y: string } {
  if (publicKey.length !== P256_PUBLIC_KEY_BYTES || publicKey[0] !== 0x04) {
    throw new Error("expected an uncompressed P-256 public key of 65 bytes");
  }
  return {
    x: base64UrlEncode(publicKey.slice(1, 33)),
    y: base64UrlEncode(publicKey.slice(33, 65)),
  };
}

/**
 * Encrypt a payload to a browser's subscription keys.
 *
 * The result is a complete `aes128gcm` body: the header carries the salt and
 * the throwaway public key the browser needs to derive the same secret, and
 * the single record that follows carries the message.
 */
export async function encryptPushPayload(
  subscription: Pick<PushSubscription, "p256dh" | "auth">,
  plaintext: Uint8Array,
  options: { salt?: Uint8Array } = {},
): Promise<Uint8Array> {
  // The header counts against the size the push services accept, so it must
  // count against the guard too.
  if (HEADER_BYTES + plaintext.length + RECORD_OVERHEAD > RECORD_SIZE) {
    throw new Error("push payload is too large for one record");
  }

  const userAgentPublicKey = base64UrlDecode(subscription.p256dh);
  const authSecret = base64UrlDecode(subscription.auth);
  const salt = options.salt ?? crypto.getRandomValues(new Uint8Array(SALT_BYTES));

  // A key pair for this message alone. The browser needs the public half to
  // reach the same shared secret, so it travels in the header.
  const ephemeral = (await crypto.subtle.generateKey({ name: "ECDH", namedCurve: "P-256" }, true, [
    "deriveBits",
  ])) as CryptoKeyPair;
  const ephemeralPublicKey = new Uint8Array(
    await crypto.subtle.exportKey("raw", ephemeral.publicKey),
  );

  const recipient = await crypto.subtle.importKey(
    "raw",
    userAgentPublicKey as BufferSource,
    { name: "ECDH", namedCurve: "P-256" },
    false,
    [],
  );
  const sharedSecret = new Uint8Array(
    await crypto.subtle.deriveBits({ name: "ECDH", public: recipient }, ephemeral.privateKey, 256),
  );

  // The browser is named in the derivation, so a payload encrypted for one
  // subscription cannot be read by another.
  const keyInfo = concatBytes(
    utf8("WebPush: info"),
    Uint8Array.of(0),
    userAgentPublicKey,
    ephemeralPublicKey,
  );
  const inputKeyMaterial = await hkdf(authSecret, sharedSecret, keyInfo, 32);
  const contentEncryptionKey = await hkdf(
    salt,
    inputKeyMaterial,
    concatBytes(utf8("Content-Encoding: aes128gcm"), Uint8Array.of(0)),
    16,
  );
  const nonce = await hkdf(
    salt,
    inputKeyMaterial,
    concatBytes(utf8("Content-Encoding: nonce"), Uint8Array.of(0)),
    12,
  );

  const aesKey = await crypto.subtle.importKey(
    "raw",
    contentEncryptionKey as BufferSource,
    { name: "AES-GCM" },
    false,
    ["encrypt"],
  );
  // `0x02` marks the last record. There is only ever one here.
  const record = concatBytes(plaintext, Uint8Array.of(2));
  const ciphertext = new Uint8Array(
    await crypto.subtle.encrypt(
      { name: "AES-GCM", iv: nonce as BufferSource, tagLength: 128 },
      aesKey,
      record as BufferSource,
    ),
  );

  const header = new Uint8Array(HEADER_BYTES);
  header.set(salt, 0);
  new DataView(header.buffer).setUint32(SALT_BYTES, RECORD_SIZE, false);
  header[SALT_BYTES + 4] = P256_PUBLIC_KEY_BYTES;
  header.set(ephemeralPublicKey, SALT_BYTES + 5);

  return concatBytes(header, ciphertext);
}

/**
 * The `Authorization` header that identifies this sender to a push service.
 *
 * The signature covers the push service's own origin, so a token taken from
 * one request cannot be replayed against a different service.
 */
export async function vapidAuthorization(endpoint: string, keys: VapidKeys): Promise<string> {
  const audience = new URL(endpoint).origin;
  const header = base64UrlEncode(utf8(JSON.stringify({ typ: "JWT", alg: "ES256" })));
  const payload = base64UrlEncode(
    utf8(
      JSON.stringify({
        aud: audience,
        exp: Math.floor(Date.now() / 1000) + VAPID_EXPIRY_SECONDS,
        sub: keys.subject,
      }),
    ),
  );
  const signingInput = `${header}.${payload}`;

  const publicKey = base64UrlDecode(keys.publicKey);
  const { x, y } = pointToJwkCoordinates(publicKey);
  const signingKey = await crypto.subtle.importKey(
    "jwk",
    { kty: "EC", crv: "P-256", x, y, d: keys.privateKey, ext: true },
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["sign"],
  );
  // WebCrypto signs to the raw `r || s` pair that JWS wants, so there is no
  // DER to unwrap here.
  const signature = new Uint8Array(
    await crypto.subtle.sign(
      { name: "ECDSA", hash: "SHA-256" },
      signingKey,
      utf8(signingInput) as BufferSource,
    ),
  );

  return `vapid t=${signingInput}.${base64UrlEncode(signature)}, k=${keys.publicKey}`;
}

/** What a push service said about a subscription. */
export interface PushDeliveryResult {
  ok: boolean;
  status: number;
  /**
   * True when the push service says this subscription is finished, which is
   * the browser having dropped it. It is removed rather than retried.
   */
  isGone: boolean;
}

/**
 * Hand one encrypted message to a push service.
 *
 * A `404` or `410` is the push service reporting that the browser has thrown
 * the subscription away, which is a normal end for one and is reported apart
 * from the failures that are worth trying again.
 */
export async function sendWebPush(
  subscription: PushSubscription,
  payload: unknown,
  keys: VapidKeys,
  options: { ttlSeconds?: number } = {},
): Promise<PushDeliveryResult> {
  const body = await encryptPushPayload(subscription, utf8(JSON.stringify(payload)));
  const response = await fetch(subscription.endpoint, {
    method: "POST",
    // The endpoint comes from a browser rather than from anywhere trusted, so
    // a redirect is reported rather than followed: where a push service sends
    // this request is not somewhere it gets to choose after the fact.
    redirect: "manual",
    headers: {
      Authorization: await vapidAuthorization(subscription.endpoint, keys),
      "Content-Encoding": "aes128gcm",
      "Content-Type": "application/octet-stream",
      TTL: String(options.ttlSeconds ?? DEFAULT_PUSH_TTL_SECONDS),
    },
    body: body as BodyInit,
  });

  return {
    ok: response.ok,
    status: response.status,
    isGone: response.status === 404 || response.status === 410,
  };
}
