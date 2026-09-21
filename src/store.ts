import { create } from "zustand";
import type { AppScreen, ConnectionStatus, UserInfo, Plan, DnsStatus } from "./types";

interface AppState {
  // Navigation
  screen: AppScreen;
  setScreen: (s: AppScreen) => void;

  // Auth
  session: string | null;
  panelUrl: string;
  setSession: (s: string | null) => void;
  setPanelUrl: (u: string) => void;

  // User data
  userInfo: UserInfo | null;
  plans: Plan[];
  setUserInfo: (u: UserInfo | null) => void;
  setPlans: (p: Plan[]) => void;

  // Connection
  connectionStatus: ConnectionStatus;
  relayIp: string | null;
  dnsStatus: DnsStatus | null;
  setConnectionStatus: (s: ConnectionStatus) => void;
  setRelayIp: (ip: string | null) => void;
  setDnsStatus: (d: DnsStatus | null) => void;

  // Error
  error: string | null;
  setError: (e: string | null) => void;

  // Logout
  logout: () => void;
}

// Load persisted values from localStorage
const savedPanelUrl = typeof window !== "undefined"
  ? localStorage.getItem("peditx_panel_url") || "https://docproir.peditxcdn.ir:8443"
  : "https://docproir.peditxcdn.ir:8443";
const savedSession = typeof window !== "undefined"
  ? localStorage.getItem("peditx_session")
  : null;

export const useAppStore = create<AppState>((set) => ({
  screen: savedSession ? "dashboard" : "login",
  setScreen: (screen) => set({ screen }),

  session: savedSession,
  panelUrl: savedPanelUrl,
  setSession: (session) => {
    if (session) localStorage.setItem("peditx_session", session);
    else localStorage.removeItem("peditx_session");
    set({ session });
  },
  setPanelUrl: (panelUrl) => {
    localStorage.setItem("peditx_panel_url", panelUrl);
    set({ panelUrl });
  },

  userInfo: null,
  plans: [],
  setUserInfo: (userInfo) => set({ userInfo }),
  setPlans: (plans) => set({ plans }),

  connectionStatus: "disconnected",
  relayIp: null,
  dnsStatus: null,
  setConnectionStatus: (connectionStatus) => set({ connectionStatus }),
  setRelayIp: (relayIp) => set({ relayIp }),
  setDnsStatus: (dnsStatus) => set({ dnsStatus }),

  error: null,
  setError: (error) => set({ error }),

  logout: () => {
    localStorage.removeItem("peditx_session");
    set({
      session: null,
      screen: "login",
      userInfo: null,
      plans: [],
      connectionStatus: "disconnected",
      relayIp: null,
      error: null,
    });
  },
}));
