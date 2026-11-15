export const playerKeys = {
  all: ["players"] as const,
  profile: (userId: string) => [...playerKeys.all, "profile", userId] as const,
  history: (userId: string) => [...playerKeys.all, "history", userId] as const,
};
