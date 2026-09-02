import { z } from "zod";

/** Base64url, as the browser writes the keys it hands out. */
const base64UrlSchema = z
  .string()
  .min(1)
  .max(512)
  .regex(/^[A-Za-z0-9_-]+$/, "expected a base64url value");

/**
 * A browser's push subscription, as the site is willing to store it.
 *
 * The endpoint is checked to be an `https` URL because it is a location this
 * site later makes a request to, and a subscription is supplied by the page
 * rather than read from anywhere trusted.
 */
export const pushSubscriptionSchema = z.object({
  endpoint: z
    .url()
    .max(2048)
    .refine((value) => value.startsWith("https://"), "a push endpoint must be https"),
  p256dh: base64UrlSchema,
  auth: base64UrlSchema,
  label: z.string().trim().min(1).max(80).nullable().default(null),
});

export type PushSubscriptionInput = z.infer<typeof pushSubscriptionSchema>;

export const pushUnsubscribeSchema = z.object({
  endpoint: z.url().max(2048),
});
