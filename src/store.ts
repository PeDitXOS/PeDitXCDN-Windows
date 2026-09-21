import { create } from "zustand";
import type { ConnectionStatus, PanelData } from "./types";

interface AppState {
  connected: boolean;
  status: ConnectionStatus;
  relay_ip: string | null;
  error: string | null;
  panel_data: PanelData | null;
  setConnected: (v: boolean) => void;
  setStatus: (s: ConnectionStatus) => void;
  setRelayIp: (ip: string | null) => void;
  setError: (e: string | null) => void;
  setPanelData: (d: PanelData | null) => void;
}

export const useAppStore = create<AppState>((set) => ({
  connected: false,
  status: "disconnected",
  relay_ip: null,
  error: null,
  panel_data: null,
  setConnected: (connected) => set({ connected }),
  setStatus: (status) => set({ status }),
  setRelayIp: (relay_ip) => set({ relay_ip }),
  setError: (error) => set({ error }),
  setPanelData: (panel_data) => set({ panel_data }),
}));
