import { createFileRoute } from "@tanstack/react-router";
import { ReplayPage } from "#/replay/ReplayPage.tsx";

export const Route = createFileRoute("/")({
  component: ReplayPage,
});
