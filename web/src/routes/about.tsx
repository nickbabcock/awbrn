import { createFileRoute } from "@tanstack/react-router";
import { AboutPage } from "#/about/AboutPage.tsx";

export const Route = createFileRoute("/about")({
  component: AboutPage,
});
