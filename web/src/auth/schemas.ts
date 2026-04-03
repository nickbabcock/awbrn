import { z } from "zod";

export const authSignInSchema = z.object({
  email: z.string().email(),
  password: z.string().min(1),
});

export const authSignUpSchema = authSignInSchema.extend({
  name: z.string().trim().min(1),
});

export type AuthSignInInput = z.infer<typeof authSignInSchema>;
export type AuthSignUpInput = z.infer<typeof authSignUpSchema>;
