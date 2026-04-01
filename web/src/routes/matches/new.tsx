import { createFileRoute } from "@tanstack/react-router";
import { NewMatchPage } from "../../pages/NewMatchPage";

export const Route = createFileRoute("/matches/new")({
  component: NewMatchPage,
});
