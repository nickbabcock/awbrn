import type { SVGProps } from "react";

/**
 * Drawn on a 24px grid with a 2px stroke in currentColor. Never substitute a
 * Unicode glyph, an emoji or a pixel icon: the only pixels are the game's sprites.
 */
export type DrawnIcon = (props: SVGProps<SVGSVGElement>) => React.JSX.Element;

type StrokeAttributes = Pick<
  SVGProps<SVGSVGElement>,
  "fill" | "stroke" | "strokeWidth" | "strokeLinecap" | "strokeLinejoin" | "strokeDasharray"
>;

function drawn(name: string, art: React.JSX.Element, attributes?: StrokeAttributes): DrawnIcon {
  function DrawnIconComponent(props: SVGProps<SVGSVGElement>) {
    return (
      <svg
        aria-hidden="true"
        fill="none"
        focusable="false"
        height="1em"
        stroke="currentColor"
        strokeWidth="2"
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
  <path d="M3.5 10.5 12 3.5l8.5 7V20.5h-5.5v-6h-6v6H3.5z" />,
  { strokeLinejoin: "round" },
);

export const IconPlay = drawn(
  "IconPlay",
  <path d="M14.5 17.5 3 6V3h3l11.5 11.5M13 19l6-6M16 16l4 4M19 21l2-2M14.5 6.5 18 3h3v3l-3.5 3.5M5 14l4 4M7 17l-3 3M3 19l2 2" />,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconMail = drawn(
  "IconMail",
  <>
    <rect x="2.5" y="5" width="19" height="14" rx="2" />
    <path d="m3 6.5 9 6.5 9-6.5" />
  </>,
  { strokeLinejoin: "round" },
);

export const IconMap = drawn(
  "IconMap",
  <>
    <path d="M3 6.5 9 4l6 2.5L21 4v13.5L15 20l-6-2.5L3 20z" />
    <path d="M9 4v13.5M15 6.5V20" />
  </>,
  { strokeLinejoin: "round" },
);

export const IconReplay = drawn(
  "IconReplay",
  <>
    <path d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1L3.5 8.5" />
    <path d="M3.5 3.5v5h5M12 7.5V12l3 2" />
  </>,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconPlus = drawn("IconPlus", <path d="M12 5v14M5 12h14" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
});

export const IconCalculator = drawn(
  "IconCalculator",
  <>
    <rect x="5" y="2.5" width="14" height="19" rx="2" />
    <path d="M8.5 6.5h7M8.5 11h.01M12 11h.01M15.5 11h.01M8.5 14.5h.01M12 14.5h.01M15.5 14.5v3M8.5 18h.01M12 18h.01" />
  </>,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconFullScreen = drawn(
  "IconFullScreen",
  <path d="M8 3.5H3.5V8M16 3.5h4.5V8M8 20.5H3.5V16M16 20.5h4.5V16" />,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconMore = drawn(
  "IconMore",
  <>
    <circle cx="5" cy="12" r="2" />
    <circle cx="12" cy="12" r="2" />
    <circle cx="19" cy="12" r="2" />
  </>,
  { fill: "currentColor", stroke: "none" },
);

export const IconBack = drawn("IconBack", <path d="M15 5 8 12l7 7" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

export const IconNext = drawn("IconNext", <path d="m9 5 7 7-7 7" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

export const IconChevronDown = drawn("IconChevronDown", <path d="m6 9 6 6 6-6" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

export const IconFirst = drawn("IconFirst", <path d="M6 5v14M18 5l-8 7 8 7" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

export const IconLast = drawn("IconLast", <path d="M18 5v14M6 5l8 7-8 7" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

export const IconTurnBack = drawn("IconTurnBack", <path d="m12 5-7 7 7 7M20 5l-7 7 7 7" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

export const IconTurnNext = drawn("IconTurnNext", <path d="m4 5 7 7-7 7M12 5l7 7-7 7" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

export const IconCheck = drawn("IconCheck", <path d="m5 12.5 4.5 4.5L19 7.5" />, {
  strokeWidth: "2.6",
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

export const IconClose = drawn("IconClose", <path d="M6 6l12 12M18 6 6 18" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
});

export const IconUpload = drawn(
  "IconUpload",
  <path d="M12 15V3.5M7 8l5-4.5L17 8M4 15v4.5h16V15" />,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconDownload = drawn(
  "IconDownload",
  <path d="M12 3.5V15M7 10.5l5 4.5 5-4.5M4 15v4.5h16V15" />,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconBell = drawn(
  "IconBell",
  <path d="M6 9.5a6 6 0 0 1 12 0c0 6 2.5 7.5 2.5 7.5h-17S6 15.5 6 9.5M10 20.5a2.2 2.2 0 0 0 4 0" />,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconEye = drawn(
  "IconEye",
  <>
    <path d="M2 12s3.5-6.5 10-6.5S22 12 22 12s-3.5 6.5-10 6.5S2 12 2 12z" />
    <circle cx="12" cy="12" r="3" />
  </>,
  { strokeLinejoin: "round" },
);

export const IconFog = drawn("IconFog", <path d="M4 8h11M7 12h13M3 16h10M15 16h5M6 20h9" />, {
  strokeLinecap: "round",
});

export const IconLock = drawn(
  "IconLock",
  <>
    <rect x="4.5" y="10.5" width="15" height="10" rx="2" />
    <path d="M8 10.5V7.5a4 4 0 0 1 8 0v3" />
  </>,
  { strokeLinejoin: "round" },
);

export const IconFlag = drawn("IconFlag", <path d="M5 21V4M5 4.5h11l-2 4 2 4H5" />, {
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

export const IconRefresh = drawn(
  "IconRefresh",
  <path d="M20 12a8 8 0 1 1-2.4-5.7L20 8.5M20 3.5v5h-5" />,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconLink = drawn(
  "IconLink",
  <path d="M10 14a4.5 4.5 0 0 0 6.4 0l3-3a4.5 4.5 0 0 0-6.4-6.4l-1 1M14 10a4.5 4.5 0 0 0-6.4 0l-3 3a4.5 4.5 0 0 0 6.4 6.4l1-1" />,
  { strokeLinecap: "round" },
);

export const IconFilter = drawn(
  "IconFilter",
  <>
    <path d="M4 7h10M18 7h2M4 17h4M12 17h8" />
    <circle cx="16" cy="7" r="2" />
    <circle cx="10" cy="17" r="2" />
  </>,
  { strokeLinecap: "round" },
);

export const IconMinus = drawn("IconMinus", <path d="M5 12h14" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
});

export const IconWholeMap = drawn(
  "IconWholeMap",
  <>
    <path d="M3.5 8V3.5H8M16 3.5h4.5V8M20.5 16v4.5H16M8 20.5H3.5V16" />
    <rect x="8" y="8" width="8" height="8" rx="1" />
  </>,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconLog = drawn(
  "IconLog",
  <>
    <path d="M9 6h11M9 12h11M9 18h11" />
    <circle cx="4.5" cy="6" r="1.2" fill="currentColor" />
    <circle cx="4.5" cy="12" r="1.2" fill="currentColor" />
    <circle cx="4.5" cy="18" r="1.2" fill="currentColor" />
  </>,
  { strokeLinecap: "round" },
);

export const IconRoute = drawn(
  "IconRoute",
  <>
    <circle cx="5.5" cy="18.5" r="2.5" />
    <circle cx="18.5" cy="5.5" r="2.5" />
    <path d="M8 18.5h7.5a3.5 3.5 0 0 0 0-7h-7a3.5 3.5 0 0 1 0-7H16" />
  </>,
  { strokeLinecap: "round", strokeLinejoin: "round" },
);

export const IconOpenSeat = drawn(
  "IconOpenSeat",
  <rect x="4" y="4" width="16" height="16" rx="1.5" />,
  { strokeDasharray: "3 2.4" },
);

export const IconChevronUp = drawn("IconChevronUp", <path d="m6 15 6-6 6 6" />, {
  strokeWidth: "2.2",
  strokeLinecap: "round",
  strokeLinejoin: "round",
});

/** A busy key turns it a spoke at a time (`steps(8)`). */
export const IconSpinner = drawn(
  "IconSpinner",
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
  { strokeLinecap: "round" },
);
