import { Checkbox } from "@base-ui/react/checkbox";
import { Popover } from "@base-ui/react/popover";
import { ScrollArea } from "@base-ui/react/scroll-area";
import * as stylex from "@stylexjs/stylex";
import { useId, useState } from "react";
import type { ComponentPropsWithoutRef, ReactNode } from "react";
import type { CoPortraitEntry } from "#/components/co_portraits.ts";
import { CoPortrait } from "#/components/CoPortrait.tsx";
import { tokens } from "./theme.stylex";
import { sx, type XStyle } from "./stylex";
import { Stack } from "./layout";
import { Text } from "./typography";

const labelStyles = stylex.create({
  base: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontPixel,
    fontSize: 9,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
});

const inputStyles = stylex.create({
  base: {
    width: "100%",
    minHeight: 44,
    paddingInline: tokens.space3,
    borderWidth: 3,
    borderStyle: "solid",
    borderColor: {
      default: tokens.strokeBase,
      ":focus": tokens.focusRing,
    },
    borderRadius: tokens.radius2,
    backgroundColor: {
      default: tokens.panelRaised,
      ":focus": "#fffbed",
    },
    boxShadow: {
      default: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
      ":focus": `${tokens.highlightInset}, ${tokens.shadowHardMd}`,
    },
    color: {
      default: tokens.inkStrong,
      "::placeholder": tokens.inkMuted,
    },
    fontFamily: tokens.fontBody,
    fontSize: tokens.textBase,
    outline: "none",
    transitionDuration: tokens.transitionFast,
    transitionProperty: "border-color, background-color, box-shadow, transform",
    transform: {
      default: "translateY(0)",
      ":focus": "translate(-1px, -1px)",
    },
  },
  select: {
    appearance: "none",
    backgroundImage: `linear-gradient(45deg, transparent 50%, ${tokens.lineSoft} 50%), linear-gradient(135deg, ${tokens.lineSoft} 50%, transparent 50%)`,
    backgroundPosition: "calc(100% - 18px) calc(50% - 2px), calc(100% - 12px) calc(50% - 2px)",
    backgroundRepeat: "no-repeat",
    backgroundSize: "6px 6px, 6px 6px",
    paddingInlineEnd: 40,
  },
});

const checkboxStyles = stylex.create({
  row: {
    display: "inline-flex",
    alignItems: "center",
    gap: tokens.space3,
    cursor: "pointer",
  },
  root: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: 22,
    height: 22,
    borderWidth: 3,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius1,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
  },
  indicator: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: "100%",
    height: "100%",
    backgroundColor: tokens.success,
    color: tokens.onDarkStrong,
    fontFamily: tokens.fontPixel,
    fontSize: 10,
    borderRadius: 2,
    boxShadow: tokens.highlightInset,
  },
  label: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontBody,
    fontSize: tokens.textBase,
  },
});

const coPickerStyles = stylex.create({
  trigger: {
    width: "100%",
    minHeight: 80,
    justifyContent: "space-between",
    paddingBlock: tokens.space3,
    paddingInline: tokens.space3,
    borderWidth: 3,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius3,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardMd}`,
    color: tokens.inkStrong,
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    gap: tokens.space3,
    textAlign: "left",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
    },
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, border-color",
  },
  caret: {
    fontFamily: tokens.fontPixel,
    fontSize: 10,
    color: tokens.inkMuted,
  },
  copy: {
    display: "grid",
    gap: 4,
    minWidth: 0,
    flex: 1,
  },
  title: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontBody,
    fontSize: tokens.textBase,
    fontWeight: 700,
    whiteSpace: "nowrap",
    overflow: "hidden",
    textOverflow: "ellipsis",
  },
  meta: {
    color: tokens.inkMuted,
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
  popup: {
    width: "min(640px, calc(100vw - 32px))",
    borderWidth: 3,
    borderStyle: "solid",
    borderColor: tokens.strokeHeavy,
    borderRadius: tokens.radius3,
    backgroundColor: tokens.panelRaised,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardLg}`,
    padding: tokens.space3,
  },
  viewport: {
    maxHeight: "min(420px, 60vh)",
  },
  grid: {
    display: "grid",
    gap: tokens.space2,
    gridTemplateColumns: {
      default: "repeat(auto-fill, minmax(120px, 1fr))",
      "@media (max-width: 640px)": "repeat(2, minmax(0, 1fr))",
    },
  },
  tile: {
    display: "grid",
    gap: tokens.space2,
    alignContent: "start",
    padding: tokens.space2,
    borderWidth: 2,
    borderStyle: "solid",
    borderColor: tokens.strokeBase,
    borderRadius: tokens.radius2,
    backgroundColor: tokens.panelBg,
    boxShadow: `${tokens.highlightInset}, ${tokens.shadowHardSm}`,
    textAlign: "left",
    cursor: "pointer",
    transform: {
      default: "translateY(0)",
      ":hover": "translate(-1px, -1px)",
      ":active": `translate(${tokens.pressOffsetSm}, ${tokens.pressOffsetSm})`,
    },
    transitionDuration: tokens.transitionFast,
    transitionProperty: "transform, box-shadow, border-color, background-color",
  },
  selected: {
    borderColor: tokens.strokeHeavy,
    backgroundColor: tokens.brandSoft,
  },
  name: {
    color: tokens.inkStrong,
    fontFamily: tokens.fontBody,
    fontSize: tokens.textSm,
    fontWeight: 700,
  },
  tileMeta: {
    color: tokens.inkMuted,
    fontFamily: tokens.fontPixel,
    fontSize: 8,
    letterSpacing: tokens.trackingPixel,
    textTransform: "uppercase",
  },
});

export function TextField({
  label,
  helper,
  error,
  xstyle,
  ...props
}: ComponentPropsWithoutRef<"input"> & {
  label: ReactNode;
  helper?: ReactNode;
  error?: ReactNode;
  xstyle?: XStyle;
}) {
  return (
    <Stack gap="xs" xstyle={xstyle}>
      <label {...sx(labelStyles.base)}>
        <Stack gap="xs">
          <span>{label}</span>
          <input {...props} {...sx(inputStyles.base)} />
        </Stack>
      </label>
      {helper ? (
        <Text size="sm" tone="muted">
          {helper}
        </Text>
      ) : null}
      {error ? (
        <Text size="sm" tone="danger">
          {error}
        </Text>
      ) : null}
    </Stack>
  );
}

export function SelectField({
  label,
  helper,
  error,
  options,
  xstyle,
  ...props
}: Omit<ComponentPropsWithoutRef<"select">, "children"> & {
  label: ReactNode;
  helper?: ReactNode;
  error?: ReactNode;
  options: ReadonlyArray<{ value: string | number; label: ReactNode }>;
  xstyle?: XStyle;
}) {
  return (
    <Stack gap="xs" xstyle={xstyle}>
      <label {...sx(labelStyles.base)}>
        <Stack gap="xs">
          <span>{label}</span>
          <select {...props} {...sx(inputStyles.base, inputStyles.select)}>
            {options.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </Stack>
      </label>
      {helper ? (
        <Text size="sm" tone="muted">
          {helper}
        </Text>
      ) : null}
      {error ? (
        <Text size="sm" tone="danger">
          {error}
        </Text>
      ) : null}
    </Stack>
  );
}

export function CheckboxField({
  label,
  helper,
  error,
  checked,
  onChange,
  disabled,
}: {
  label: ReactNode;
  helper?: ReactNode;
  error?: ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
}) {
  const id = useId();

  return (
    <Stack gap="xs">
      <label htmlFor={id} {...sx(checkboxStyles.row)}>
        <Checkbox.Root
          checked={checked}
          disabled={disabled}
          id={id}
          onCheckedChange={onChange}
          {...sx(checkboxStyles.root)}
        >
          <Checkbox.Indicator keepMounted {...sx(checkboxStyles.indicator)}>
            {checked ? "✓" : ""}
          </Checkbox.Indicator>
        </Checkbox.Root>
        <span {...sx(checkboxStyles.label)}>{label}</span>
      </label>
      {helper ? (
        <Text size="sm" tone="muted">
          {helper}
        </Text>
      ) : null}
      {error ? (
        <Text size="sm" tone="danger">
          {error}
        </Text>
      ) : null}
    </Stack>
  );
}

export function CoPickerField({
  label,
  helper,
  error,
  value,
  options,
  disabled,
  onChange,
}: {
  label: ReactNode;
  helper?: ReactNode;
  error?: ReactNode;
  value: number | null;
  options: ReadonlyArray<CoPortraitEntry>;
  disabled?: boolean;
  onChange: (value: number | null) => void;
}) {
  const selected = options.find((option) => option.awbwId === value) ?? null;
  const [open, setOpen] = useState(false);
  const fieldId = useId();
  const labelId = `${fieldId}-label`;
  const helperTextId = helper ? `${fieldId}-helper` : undefined;
  const errorTextId = error ? `${fieldId}-error` : undefined;
  const describedBy =
    [helperTextId, errorTextId].filter((value): value is string => value !== undefined).join(" ") ||
    undefined;

  return (
    <Stack gap="xs">
      <label id={labelId} {...sx(labelStyles.base)}>
        {label}
      </label>
      <Popover.Root open={open} onOpenChange={setOpen}>
        <Popover.Trigger
          aria-describedby={describedBy}
          aria-invalid={!!error}
          aria-labelledby={labelId}
          disabled={disabled}
          {...sx(coPickerStyles.trigger)}
        >
          <CoPortrait catalog={null} coKey={selected?.key ?? null} fallbackLabel="?" />
          <div {...sx(coPickerStyles.copy)}>
            <span {...sx(coPickerStyles.title)}>{selected?.displayName ?? "No CO selected"}</span>
            <span {...sx(coPickerStyles.meta)}>Portrait selector</span>
          </div>
          <span aria-hidden="true" {...sx(coPickerStyles.caret)}>
            ▼
          </span>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Positioner align="start" sideOffset={10}>
            <Popover.Popup initialFocus={false} {...sx(coPickerStyles.popup)}>
              <ScrollArea.Root>
                <ScrollArea.Viewport {...sx(coPickerStyles.viewport)}>
                  <ScrollArea.Content>
                    <div {...sx(coPickerStyles.grid)}>
                      <button
                        {...sx(coPickerStyles.tile, value === null && coPickerStyles.selected)}
                        onClick={() => {
                          onChange(null);
                          setOpen(false);
                        }}
                        type="button"
                      >
                        <div {...sx(coPickerStyles.name)}>No CO</div>
                        <div {...sx(coPickerStyles.tileMeta)}>Clear selection</div>
                      </button>
                      {options.map((option) => (
                        <button
                          key={option.awbwId}
                          {...sx(
                            coPickerStyles.tile,
                            option.awbwId === value && coPickerStyles.selected,
                          )}
                          onClick={() => {
                            onChange(option.awbwId);
                            setOpen(false);
                          }}
                          type="button"
                        >
                          <CoPortrait catalog={null} coKey={option.key} fallbackLabel="?" />
                          <div {...sx(coPickerStyles.name)}>{option.displayName}</div>
                          <div {...sx(coPickerStyles.tileMeta)}>CO #{option.awbwId}</div>
                        </button>
                      ))}
                    </div>
                  </ScrollArea.Content>
                </ScrollArea.Viewport>
              </ScrollArea.Root>
            </Popover.Popup>
          </Popover.Positioner>
        </Popover.Portal>
      </Popover.Root>
      {helper ? (
        <Text id={helperTextId} size="sm" tone="muted">
          {helper}
        </Text>
      ) : null}
      {error ? (
        <Text id={errorTextId} size="sm" tone="danger">
          {error}
        </Text>
      ) : null}
    </Stack>
  );
}
