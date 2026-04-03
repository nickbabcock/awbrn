import { createFileRoute } from "@tanstack/react-router";
import { NewMatchPage } from "../../matches/screens/NewMatchPage";

export const Route = createFileRoute("/matches/new")({
  component: NewMatchPage,
});
