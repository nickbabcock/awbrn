import type { ComponentPropsWithoutRef } from "react";
import "@astryxdesign/core/TextInput";

declare module "@astryxdesign/core/TextInput" {
  interface TextInputProps {
    /** Native input autocomplete hint; TextInput forwards unknown input props. */
    autoComplete?: ComponentPropsWithoutRef<"input">["autoComplete"];
  }
}
