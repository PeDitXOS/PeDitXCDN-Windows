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

/** One config the pasted body became, and whether its switch is on. */
export interface TunnelCfg {
  tag: string;
  enabled: boolean;
}

/** Which config one ticked app rides. Only ever a tag from `outbounds`. */
export interface TunnelRoute {
  path: string;
  tag: string;
}

/** sing-box sidecar, as the backend sees it. */
export interface TunnelStatus {
  running: boolean;
  has_config: boolean;
  apps: string[];
  /** The configs the body became — listed beside the apps. */
  outbounds: TunnelCfg[];
  /** Per-app choices; an app with no row rides the first config that is on. */
  routes: TunnelRoute[];
  error: string | null;
  controller: string;
}

/** One live clash_api connection — proof a process is actually tunnelling. */
export interface TunnelConn {
  process: string;
  path: string;
  chains: string[];
}

/** The knobs the settings tab exposes. Scope (which apps) is *not* here —
 *  nothing on this page can widen it. */
export interface TunnelOpts {
  mtu: number;
  auto_route: boolean;
  strict_route: boolean;
  dns_mode: "disabled" | "native" | "hijack";
  /** Extra CIDRs kept out of the TUN, one per line. */
  exclude: string;
  /** Extra route rules, raw JSON `[ { … } ]`. */
  extra_rules: string;
  log_level: string;
  clash_secret: string;
}
