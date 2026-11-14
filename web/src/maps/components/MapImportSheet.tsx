import { Banner } from "@astryxdesign/core/Banner";
import { Icon } from "@astryxdesign/core/Icon";
import { Stack } from "@astryxdesign/core/Stack";
import { TextInput } from "@astryxdesign/core/TextInput";
import * as stylex from "@stylexjs/stylex";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { importFailureMessage } from "#/maps/map_import.ts";
import { importAwbwMapFn } from "#/maps/maps.functions.ts";
import { mapKeys } from "#/maps/maps.keys.ts";
import { Button } from "#/ui/Button.tsx";
import { IconDownload } from "#/ui/icons.tsx";
import { ModalSheet } from "#/ui/ModalSheet.tsx";

export function MapImportKey() {
  const [isOpen, setIsOpen] = useState(false);
  // The form stays mounted so the sheet leaves with it, and starts fresh on
  // each opening.
  const [opening, setOpening] = useState(0);

  return (
    <>
      <Button
        icon={<Icon icon={IconDownload} />}
        label="Import a map"
        onClick={() => {
          setOpening((count) => count + 1);
          setIsOpen(true);
        }}
        variant="secondary"
      />
      <ModalSheet isOpen={isOpen} onOpenChange={setIsOpen} purpose="form" title="Import a map">
        <ImportForm key={opening} />
      </ModalSheet>
    </>
  );
}

function ImportForm() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [typed, setTyped] = useState("");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: importAwbwMapFn,
    onSuccess: async (map) => {
      await queryClient.invalidateQueries({ queryKey: mapKeys.all });
      await navigate({ params: { mapId: map.mapId }, to: "/maps/$mapId" });
    },
  });

  async function handleSubmit(event: React.FormEvent): Promise<void> {
    event.preventDefault();
    const sourceMapId = Number(typed.trim());
    if (!/^\d+$/.test(typed.trim()) || sourceMapId <= 0) {
      setError("Enter the map's AWBW id");
      return;
    }
    setError(null);
    try {
      await mutation.mutateAsync({ data: { sourceMapId } });
    } catch (importError) {
      setError(importFailureMessage(importError));
    }
  }

  return (
    <Stack as="form" onSubmit={(event) => void handleSubmit(event)} xstyle={styles.form}>
      <TextInput
        autoComplete="off"
        enterKeyHint="go"
        hasAutoFocus
        inputMode="numeric"
        label="AWBW map ID"
        onChange={(value) => {
          setError(null);
          setTyped(value);
        }}
        placeholder="162795"
        value={typed}
      />
      {error ? <Banner role="alert" status="error" title={error} /> : null}
      <Button
        isLoading={mutation.isPending}
        label="Import"
        size="lg"
        type="submit"
        variant="primary"
        width="100%"
      />
    </Stack>
  );
}

const styles = stylex.create({
  form: {
    display: "grid",
    gap: "16px",
    padding: "20px",
  },
});
