import { HStack } from "@astryxdesign/core/Stack";
import { TextInput } from "@astryxdesign/core/TextInput";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { authKeys } from "#/auth/auth.keys.ts";
import { authClient } from "#/auth/client.ts";
import { displayNameSchema } from "#/auth/display_name.ts";
import { Button } from "#/ui/Button.tsx";
import { playerKeys } from "../players.keys.ts";

/** The name field that stands in for the heading while a player renames. */
export function ProfileRename({ name, onDone }: { name: string; onDone: () => void }) {
  const queryClient = useQueryClient();
  const router = useRouter();
  const [value, setValue] = useState(name);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    const parsed = displayNameSchema.safeParse(value);
    if (!parsed.success) {
      setError(parsed.error.issues[0]?.message ?? "Invalid name");
      return;
    }
    if (parsed.data === name) {
      onDone();
      return;
    }

    const result = await authClient.updateUser({ name: parsed.data });
    if (result.error) {
      setError(result.error.message ?? "Rename failed");
      return;
    }
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: authKeys.all }),
      queryClient.invalidateQueries({ queryKey: playerKeys.all }),
    ]);
    await router.invalidate();
    onDone();
  }

  return (
    <HStack align="start" gap={2} xstyle={styles.row}>
      <TextInput
        autoComplete="nickname"
        hasAutoFocus
        isLabelHidden
        label="Name"
        onChange={(next) => {
          setValue(next);
          setError(null);
        }}
        onEnter={save}
        status={error ? { type: "error", message: error } : undefined}
        type="text"
        value={value}
        containerXstyle={styles.field}
      />
      <Button clickAction={save} label="Save" variant="primary" />
      <Button clickAction={onDone} label="Cancel" variant="ghost" />
    </HStack>
  );
}

const styles = stylex.create({
  row: {
    minInlineSize: 0,
  },
  field: {
    minInlineSize: 0,
    maxInlineSize: "280px",
  },
});
