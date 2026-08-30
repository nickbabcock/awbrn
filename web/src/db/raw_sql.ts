/*
 * Types for the queries which stay on raw SQL.
 *
 * The conditional inserts which make the matchmaking pass idempotent are not
 * expressible with Drizzle, so they keep their SQL. Their row types still come
 * from the Drizzle schema, so a column rename breaks the build instead of
 * returning `undefined` at run time.
 */

/**
 * The type D1 returns for one column value.
 *
 * Drizzle decodes a `timestamp` column to a `Date` and a `boolean` column to a
 * `boolean`. The raw driver returns the stored integer, so the column types
 * must go back to the terms SQLite stores them in.
 */
type RawColumn<TValue> = TValue extends Date ? number : TValue extends boolean ? number : TValue;

/** The type D1 returns for one column of a Drizzle table row. */
export type RawCol<TRow, TKey extends keyof TRow> = RawColumn<TRow[TKey]>;

/** Selected columns of a Drizzle table row, in the types D1 returns them. */
export type RawRow<TRow, TKeys extends keyof TRow> = { [K in TKeys]: RawColumn<TRow[K]> };
