export type AppScreen = "login" | "dashboard";
export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "error";

export interface LoginResponse {
  ok: boolean;
  session?: string;
  message?: string;
}

export interface UserInfo {
  ok: boolean;
  name?: string;
  telegram_id?: number;
  ip?: string;
  used?: number;
  quota?: number;
  status?: string;
  wallet?: number;
  plan?: string;
  plan_name?: string;
  renews?: string;
  expires?: string;
  speed_kbps?: number;
  speed_mbps?: number;
  days_left?: number;
  gb_used?: number;
  gb_total?: number;
  warned?: unknown;
  seen_ip?: string;
}

export interface Plan {
  id: number;
  name: string;
  price: number;
  desc?: string;
  days?: number;
  gb?: number;
  mbps?: number;
}

export interface PlansResponse {
  ok: boolean;
  plans?: Plan[];
  current?: number;
}

export interface SimpleResponse {
  ok: boolean;
  message?: string;
}

export interface DnsStatus {
  configured: boolean;
  current_dns?: string;
  interface: string;
  is_relay_dns: boolean;
}
