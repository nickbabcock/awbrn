// Tailored from Nano ID for this codebase: TypeScript, ESM, and Web Crypto only.
export const urlAlphabet = "useandom-26T198340PX75pxmralfivszgbkhjopqywUVXYQWJKZHFcdeCDEGILMNRSOqt";

const POOL_SIZE_MULTIPLIER = 128;
const MAX_RANDOM_VALUES_SIZE = 65_536;

let pool: Uint8Array | null = null;
let poolOffset = 0;

function fillPool(bytes: number): void {
  if (pool === null || pool.length < bytes) {
    pool = new Uint8Array(bytes * POOL_SIZE_MULTIPLIER);
    fillRandomValues(pool);
    poolOffset = 0;
  } else if (poolOffset + bytes > pool.length) {
    fillRandomValues(pool);
    poolOffset = 0;
  }

  poolOffset += bytes;
}

function fillRandomValues(buffer: Uint8Array): void {
  for (let offset = 0; offset < buffer.length; offset += MAX_RANDOM_VALUES_SIZE) {
    crypto.getRandomValues(buffer.subarray(offset, offset + MAX_RANDOM_VALUES_SIZE));
  }
}

export function random(bytes: number): Uint8Array {
  fillPool((bytes |= 0));
  return pool!.subarray(poolOffset - bytes, poolOffset);
}

export function customRandom(
  alphabet: string,
  defaultSize: number,
  getRandom: (bytes: number) => Uint8Array,
): (size?: number) => string {
  const mask = (2 << (31 - Math.clz32((alphabet.length - 1) | 1))) - 1;
  const step = Math.ceil((1.6 * mask * defaultSize) / alphabet.length);

  return (size = defaultSize) => {
    if (!size) {
      return "";
    }

    let id = "";

    while (true) {
      const bytes = getRandom(step);
      let index = step;

      while (index--) {
        id += alphabet[bytes[index] & mask] ?? "";
        if (id.length >= size) {
          return id;
        }
      }
    }
  };
}

export function customAlphabet(alphabet: string, size = 21): (size?: number) => string {
  return customRandom(alphabet, size, random);
}

export function nanoid(size = 21): string {
  fillPool((size |= 0));

  let id = "";

  for (let index = poolOffset - size; index < poolOffset; index += 1) {
    id += urlAlphabet[pool![index] & 63];
  }

  return id;
}
