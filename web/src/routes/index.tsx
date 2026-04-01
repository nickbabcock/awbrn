import { createFileRoute } from "@tanstack/react-router";
import { ReplayPage } from "../pages/ReplayPage";

export const Route = createFileRoute("/")({
  component: ReplayPage,
});
