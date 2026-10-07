import type { ReactNode } from "react";

type IconName =
  | "home"
  | "plus"
  | "settings"
  | "search"
  | "mic"
  | "speaker"
  | "screen"
  | "back"
  | "arrow"
  | "calendar"
  | "clock"
  | "file"
  | "download"
  | "trash"
  | "play"
  | "info"
  | "check";

const shapes: Record<IconName, ReactNode> = {
  home: <path d="m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1z" />,
  plus: <path d="M12 5v14M5 12h14" />,
  settings: (
    <>
      <path d="M12 3.5v2m0 13v2M3.5 12h2m13 0h2M6 6l1.5 1.5m9 9L18 18M18 6l-1.5 1.5m-9 9L6 18" />
      <circle cx="12" cy="12" r="4" />
    </>
  ),
  search: (
    <>
      <circle cx="10.8" cy="10.8" r="6.8" />
      <path d="m16 16 5 5" />
    </>
  ),
  mic: (
    <>
      <rect x="9" y="3" width="6" height="12" rx="3" />
      <path d="M6 11a6 6 0 0 0 12 0M12 17v4m-4 0h8" />
    </>
  ),
  speaker: (
    <path d="M4 9v6h4l5 4V5L8 9H4Zm12-1a6 6 0 0 1 0 8m2-11a10 10 0 0 1 0 14" />
  ),
  screen: <path d="M3 4h18v13H3zM8 21h8m-4-4v4" />,
  back: <path d="m15 18-6-6 6-6M9 12h12" />,
  arrow: <path d="m9 5 7 7-7 7" />,
  calendar: <path d="M4 6h16v15H4zM8 3v6m8-6v6M4 11h16" />,
  clock: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </>
  ),
  file: <path d="M5 3h9l5 5v13H5zM14 3v5h5M8 13h8m-8 4h8" />,
  download: <path d="M12 3v12m-4-4 4 4 4-4M4 18v3h16v-3" />,
  trash: <path d="M4 7h16M9 4h6m-9 3 1 14h10l1-14M10 11v6m4-6v6" />,
  play: <path d="m9 6 9 6-9 6z" />,
  info: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v6m0-10h.01" />
    </>
  ),
  check: <path d="m4 12 5 5L20 6" />,
};

export function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {shapes[name]}
    </svg>
  );
}
