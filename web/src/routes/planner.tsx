import { createFileRoute } from "@tanstack/react-router";
import { PlannerPage } from "#/planner/PlannerPage.tsx";

export const Route = createFileRoute("/planner")({
  component: PlannerPage,
});
