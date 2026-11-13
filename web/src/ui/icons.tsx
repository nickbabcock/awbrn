import type { SVGProps } from "react";

/**
 * The drawn icon set: one set, a 24px grid, a 2px round stroke, painted with
 * currentColor.
 *
 * Usage: pass a component to Astryx `Icon` or to a key's `icon` slot.
 *   <Icon icon={IconCalculator} size="md" />
 *   <Button icon={<Icon icon={IconPlus} />} label="New match" />
 * Icon keys (calculator, more, review, upload, close, log, deselect) are
 * icon-only with an accessible name and a tooltip; Whole map, End turn and
 * Back to live keep their words. Never a Unicode glyph or an emoji in place of
 * one of these, and never a pixel icon: the only pixels are the game's sprites.
 */
export type DrawnIcon = (props: SVGProps<SVGSVGElement>) => React.JSX.Element;

type StrokeAttributes = Pick<
  SVGProps<SVGSVGElement>,
  "fill" | "stroke" | "strokeWidth" | "strokeLinecap" | "strokeLinejoin" | "strokeDasharray"
>;

function drawn(name: string, attributes: StrokeAttributes, art: React.JSX.Element): DrawnIcon {
  function DrawnIconComponent(props: SVGProps<SVGSVGElement>) {
    return (
      <svg
        aria-hidden="true"
        focusable="false"
        height="1em"
        viewBox="0 0 24 24"
        width="1em"
        xmlns="http://www.w3.org/2000/svg"
        {...attributes}
        {...props}
      >
        {art}
      </svg>
    );
  }
  DrawnIconComponent.displayName = name;
  return DrawnIconComponent;
}

export const IconHome = drawn(
  "IconHome",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinejoin: "round" },
  <>
    <path d="M3.5 10.5 12 3.5l8.5 7V20.5h-5.5v-6h-6v6H3.5z" />
  </>,
);

export const IconPlay = drawn(
  "IconPlay",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M14.5 17.5 3 6V3h3l11.5 11.5M13 19l6-6M16 16l4 4M19 21l2-2M14.5 6.5 18 3h3v3l-3.5 3.5M5 14l4 4M7 17l-3 3M3 19l2 2" />
  </>,
);

export const IconMail = drawn(
  "IconMail",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinejoin: "round" },
  <>
    <rect x="2.5" y="5" width="19" height="14" rx="2" />
    <path d="m3 6.5 9 6.5 9-6.5" />
  </>,
);

export const IconMap = drawn(
  "IconMap",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinejoin: "round" },
  <>
    <path d="M3 6.5 9 4l6 2.5L21 4v13.5L15 20l-6-2.5L3 20z" />
    <path d="M9 4v13.5M15 6.5V20" />
  </>,
);

export const IconReplay = drawn(
  "IconReplay",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1L3.5 8.5" />
    <path d="M3.5 3.5v5h5M12 7.5V12l3 2" />
  </>,
);

export const IconPlus = drawn(
  "IconPlus",
  { fill: "none", stroke: "currentColor", strokeWidth: "2.2", strokeLinecap: "round" },
  <>
    <path d="M12 5v14M5 12h14" />
  </>,
);

export const IconSearch = drawn(
  "IconSearch",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinecap: "round" },
  <>
    <circle cx="11" cy="11" r="6.5" />
    <path d="m16 16 4.5 4.5" />
  </>,
);

export const IconCalculator = drawn(
  "IconCalculator",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <rect x="5" y="2.5" width="14" height="19" rx="2" />
    <path d="M8.5 6.5h7M8.5 11h.01M12 11h.01M15.5 11h.01M8.5 14.5h.01M12 14.5h.01M15.5 14.5v3M8.5 18h.01M12 18h.01" />
  </>,
);

export const IconFullScreen = drawn(
  "IconFullScreen",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M8 3.5H3.5V8M16 3.5h4.5V8M8 20.5H3.5V16M16 20.5h4.5V16" />
  </>,
);

export const IconMore = drawn(
  "IconMore",
  { fill: "currentColor" },
  <>
    <circle cx="5" cy="12" r="2" />
    <circle cx="12" cy="12" r="2" />
    <circle cx="19" cy="12" r="2" />
  </>,
);

export const IconBack = drawn(
  "IconBack",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2.2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M15 5 8 12l7 7" />
  </>,
);

export const IconNext = drawn(
  "IconNext",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2.2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="m9 5 7 7-7 7" />
  </>,
);

export const IconChevronDown = drawn(
  "IconChevronDown",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2.2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="m6 9 6 6 6-6" />
  </>,
);

export const IconFirst = drawn(
  "IconFirst",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2.2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M6 5v14M18 5l-8 7 8 7" />
  </>,
);

export const IconLast = drawn(
  "IconLast",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2.2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M18 5v14M6 5l8 7-8 7" />
  </>,
);

export const IconTurnBack = drawn(
  "IconTurnBack",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2.2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="m12 5-7 7 7 7M20 5l-7 7 7 7" />
  </>,
);

export const IconTurnNext = drawn(
  "IconTurnNext",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2.2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="m4 5 7 7-7 7M12 5l7 7-7 7" />
  </>,
);

export const IconCheck = drawn(
  "IconCheck",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2.6",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="m5 12.5 4.5 4.5L19 7.5" />
  </>,
);

export const IconClose = drawn(
  "IconClose",
  { fill: "none", stroke: "currentColor", strokeWidth: "2.2", strokeLinecap: "round" },
  <>
    <path d="M6 6l12 12M18 6 6 18" />
  </>,
);

export const IconUpload = drawn(
  "IconUpload",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M12 15V3.5M7 8l5-4.5L17 8M4 15v4.5h16V15" />
  </>,
);

export const IconDownload = drawn(
  "IconDownload",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M12 3.5V15M7 10.5l5 4.5 5-4.5M4 15v4.5h16V15" />
  </>,
);

export const IconBell = drawn(
  "IconBell",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M6 9.5a6 6 0 0 1 12 0c0 6 2.5 7.5 2.5 7.5h-17S6 15.5 6 9.5M10 20.5a2.2 2.2 0 0 0 4 0" />
  </>,
);

export const IconEye = drawn(
  "IconEye",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinejoin: "round" },
  <>
    <path d="M2 12s3.5-6.5 10-6.5S22 12 22 12s-3.5 6.5-10 6.5S2 12 2 12z" />
    <circle cx="12" cy="12" r="3" />
  </>,
);

export const IconFog = drawn(
  "IconFog",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinecap: "round" },
  <>
    <path d="M4 8h11M7 12h13M3 16h10M15 16h5M6 20h9" />
  </>,
);

export const IconLock = drawn(
  "IconLock",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinejoin: "round" },
  <>
    <rect x="4.5" y="10.5" width="15" height="10" rx="2" />
    <path d="M8 10.5V7.5a4 4 0 0 1 8 0v3" />
  </>,
);

export const IconFlag = drawn(
  "IconFlag",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M5 21V4M5 4.5h11l-2 4 2 4H5" />
  </>,
);

export const IconRefresh = drawn(
  "IconRefresh",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M20 12a8 8 0 1 1-2.4-5.7L20 8.5M20 3.5v5h-5" />
  </>,
);

export const IconLink = drawn(
  "IconLink",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinecap: "round" },
  <>
    <path d="M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1 1M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1-1" />
  </>,
);

export const IconUsers = drawn(
  "IconUsers",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <circle cx="9" cy="8" r="3.5" />
    <path d="M2.5 20c.8-3.6 3.3-5.5 6.5-5.5s5.7 1.9 6.5 5.5M16 4.8a3.5 3.5 0 0 1 0 6.4M18.5 14.8c1.6.8 2.6 2.5 3 5.2" />
  </>,
);

export const IconTarget = drawn(
  "IconTarget",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinecap: "round" },
  <>
    <circle cx="12" cy="12" r="6.5" />
    <circle cx="12" cy="12" r="1.6" fill="currentColor" />
    <path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3" />
  </>,
);

export const IconFilter = drawn(
  "IconFilter",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinecap: "round" },
  <>
    <path d="M4 7h10M18 7h2M4 17h4M12 17h8" />
    <circle cx="16" cy="7" r="2" />
    <circle cx="10" cy="17" r="2" />
  </>,
);

export const IconSwap = drawn(
  "IconSwap",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M4 8h14l-4-4M20 16H6l4 4" />
  </>,
);

export const IconMinus = drawn(
  "IconMinus",
  { fill: "none", stroke: "currentColor", strokeWidth: "2.2", strokeLinecap: "round" },
  <>
    <path d="M5 12h14" />
  </>,
);

export const IconWholeMap = drawn(
  "IconWholeMap",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="M3.5 8V3.5H8M16 3.5h4.5V8M20.5 16v4.5H16M8 20.5H3.5V16" />
    <rect x="8" y="8" width="8" height="8" rx="1" />
  </>,
);

export const IconLog = drawn(
  "IconLog",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinecap: "round" },
  <>
    <path d="M9 6h11M9 12h11M9 18h11" />
    <circle cx="4.5" cy="6" r="1.2" fill="currentColor" />
    <circle cx="4.5" cy="12" r="1.2" fill="currentColor" />
    <circle cx="4.5" cy="18" r="1.2" fill="currentColor" />
  </>,
);

export const IconRoute = drawn(
  "IconRoute",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <circle cx="5.5" cy="18.5" r="2.5" />
    <circle cx="18.5" cy="5.5" r="2.5" />
    <path d="M8 18.5h7.5a3.5 3.5 0 0 0 0-7h-7a3.5 3.5 0 0 1 0-7H16" />
  </>,
);

export const IconInfo = drawn(
  "IconInfo",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinecap: "round" },
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11v6M12 7.5v.01" />
  </>,
);

export const IconOpenSeat = drawn(
  "IconOpenSeat",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeDasharray: "3 2.4" },
  <>
    <rect x="4" y="4" width="16" height="16" rx="1.5" />
  </>,
);

export const IconChevronUp = drawn(
  "IconChevronUp",
  {
    fill: "none",
    stroke: "currentColor",
    strokeWidth: "2.2",
    strokeLinecap: "round",
    strokeLinejoin: "round",
  },
  <>
    <path d="m6 15 6-6 6 6" />
  </>,
);

/**
 * The busy mark: eight spokes, fading round the dial. A busy key turns it a
 * spoke at a time (`steps(8)`), so it reads as the same drawn set, not a glide.
 */
export const IconSpinner = drawn(
  "IconSpinner",
  { fill: "none", stroke: "currentColor", strokeWidth: "2", strokeLinecap: "round" },
  <>
    <path d="M12 3.5v3.5" />
    <path d="m18 6-2.5 2.5" opacity="0.3" />
    <path d="M20.5 12H17" opacity="0.35" />
    <path d="m18 18-2.5-2.5" opacity="0.45" />
    <path d="M12 20.5V17" opacity="0.55" />
    <path d="m6 18 2.5-2.5" opacity="0.65" />
    <path d="M3.5 12H7" opacity="0.8" />
    <path d="m6 6 2.5 2.5" opacity="0.9" />
  </>,
);
