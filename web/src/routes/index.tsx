import { createFileRoute } from "@tanstack/react-router";
import { ReplayPage } from "../replay/ReplayPage";

export const Route = createFileRoute("/")({
  component: ReplayPage,
});
