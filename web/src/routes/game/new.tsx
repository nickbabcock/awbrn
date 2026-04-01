import { createFileRoute } from "@tanstack/react-router";
import { NewGamePage } from "../../pages/NewGamePage";

export const Route = createFileRoute("/game/new")({
  component: NewGamePage,
});
