import { defineProject } from "vitest/config";

export default defineProject({
  test: {
    name: "node",
    environment: "node",
    include: ["src/canvas_courier/ring_buffer.test.ts"],
  },
});
