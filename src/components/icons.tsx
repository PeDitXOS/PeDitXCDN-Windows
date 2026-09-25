import type { ReactNode } from "react";

/**
 * One stroke set for the whole app: 24-grid, currentColor, 1.75 weight.
 * Nothing here is decorative-only — every icon sits next to a label, so the
 * text still carries the meaning if the glyph is missed.
 */
function Ic({ size = 16, children }: { size?: number; children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {children}
    </svg>
  );
}

/* Rail */
export const IcPower = (p: { size?: number }) => (
  <Ic {...p}><><path d="M18.36 6.64a9 9 0 1 1-12.73 0" /><path d="M12 2v10" /></></Ic>
);
export const IcUser = (p: { size?: number }) => (
  <Ic {...p}><><circle cx="12" cy="8" r="4" /><path d="M4.5 21a7.5 7.5 0 0 1 15 0" /></></Ic>
);
export const IcActivity = (p: { size?: number }) => (
  <Ic {...p}><path d="M3 12h4l2.5-7 4.5 14 2.5-7H21" /></Ic>
);
export const IcLayers = (p: { size?: number }) => (
  <Ic {...p}><><path d="m12 3 9 4.5-9 4.5-9-4.5L12 3Z" /><path d="m3 14 9 4.5L21 14" /></></Ic>
);

/* Dashboard */
export const IcCalendar = (p: { size?: number }) => (
  <Ic {...p}><><rect x="3" y="5" width="18" height="16" rx="2" /><path d="M16 3v4M8 3v4M3 10h18" /></></Ic>
);
export const IcGauge = (p: { size?: number }) => (
  <Ic {...p}><><path d="M12 14.5 16.5 10" /><path d="M3.5 18a10 10 0 1 1 17 0" /></></Ic>
);
export const IcDisk = (p: { size?: number }) => (
  <Ic {...p}><><ellipse cx="12" cy="6" rx="8" ry="3" /><path d="M4 6v6c0 1.66 3.58 3 8 3s8-1.34 8-3V6" /><path d="M4 12v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6" /></></Ic>
);
export const IcRefresh = (p: { size?: number }) => (
  <Ic {...p}><><path d="M20.5 12a8.5 8.5 0 1 1-2.5-6" /><path d="M21 3.5V9h-5.5" /></></Ic>
);
export const IcAlert = (p: { size?: number }) => (
  <Ic {...p}><><path d="M10.3 4 2.4 18a2 2 0 0 0 1.7 3h15.8a2 2 0 0 0 1.7-3L13.7 4a2 2 0 0 0-3.4 0Z" /><path d="M12 9.5v4.5M12 17.5h.01" /></></Ic>
);
export const IcCard = (p: { size?: number }) => (
  <Ic {...p}><><rect x="2" y="5" width="20" height="14" rx="2" /><path d="M2 10h20" /><path d="M6 15h4" /></></Ic>
);
export const IcChat = (p: { size?: number }) => (
  <Ic {...p}><path d="M21 11.5a8.5 8.5 0 0 1-8.5 8.5 8.4 8.4 0 0 1-3.8-.9L3 21l1.9-5.7a8.4 8.4 0 0 1-.9-3.8A8.5 8.5 0 1 1 21 11.5Z" /></Ic>
);
export const IcDown = (p: { size?: number }) => (
  <Ic {...p}><><path d="M12 3.5v11.5" /><path d="m7 10.5 5 5 5-5" /><path d="M4.5 20.5h15" /></></Ic>
);
export const IcUp = (p: { size?: number }) => (
  <Ic {...p}><><path d="M12 20.5V9" /><path d="m7 14 5-5 5 5" /><path d="M4.5 3.5h15" /></></Ic>
);
export const IcShield = (p: { size?: number }) => (
  <Ic {...p}><path d="M12 3 4.5 6v6c0 4.5 3.2 7.8 7.5 9 4.3-1.2 7.5-4.5 7.5-9V6L12 3Z" /></Ic>
);
export const IcLock = (p: { size?: number }) => (
  <Ic {...p}><><rect x="4.5" y="10.5" width="15" height="10" rx="2" /><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3" /></></Ic>
);
export const IcAt = (p: { size?: number }) => (
  <Ic {...p}><><circle cx="12" cy="12" r="4" /><path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8" /></></Ic>
);
export const IcLogin = (p: { size?: number }) => (
  <Ic {...p}><><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" /><path d="m10 17 5-5-5-5" /><path d="M15 12H3" /></></Ic>
);
export const IcGear = (p: { size?: number }) => (
  <Ic {...p}><><circle cx="12" cy="12" r="3.2" /><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09A1.7 1.7 0 0 0 8.9 19.3a1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.7 15a1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.7 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.7 1.7 0 0 0 9 4.7a1.7 1.7 0 0 0 1-1.56V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 19.3 9v.09A1.7 1.7 0 0 0 21 10.1a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.51 1Z" /></></Ic>
);
export const IcLogout = (p: { size?: number }) => (
  <Ic {...p}><><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" /><path d="m16 17 5-5-5-5" /><path d="M21 12H9" /></></Ic>
);
