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
  ipv6_dns?: string;
  interface: string;
  is_relay_dns: boolean;
}

/** Backend's view of the proxy — polled, never assumed. */
export interface ProxyStatus {
  running: boolean;
  relay?: string | null;
  uptime_secs: number;
  v6: boolean;
  fragment: boolean;
}

/** What the emergency cut actually did, read back from the system. */
export interface EmergencyStop {
  proxy_was_running: boolean;
  dns_restored: boolean;
  current_dns?: string | null;
  ipv6_dns?: string | null;
}
