export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "error";

export interface PanelData {
  relay_ip: string;
  relay_port: number;
  sub_domain: string;
  panel_url: string;
  quota_used: number;
  quota_total: number;
  expire_date: string;
}

export interface ConnectionState {
  status: ConnectionStatus;
  relay_ip: string | null;
  error: string | null;
  panel_data: PanelData | null;
}

export interface DnsRecord {
  domain: string;
  record_type: string;
  value: string;
  ttl: number;
}
