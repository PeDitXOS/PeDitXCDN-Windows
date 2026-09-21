import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import { ConnectButton } from "./ConnectButton";
import { StatusCard } from "./StatusCard";
import { QuotaBar } from "./QuotaBar";
import { PlansList } from "./PlansList";

export function Dashboard() {
  const {
    session, panelUrl, userInfo, setUserInfo,
    setPlans, setRelayIp, setDnsStatus,
    setConnectionStatus, logout, setError, setScreen,
  } = useAppStore();

  // Fetch user data on mount
  useEffect(() => {
    if (!session) {
      setScreen("login");
      return;
    }

    const fetchData = async () => {
      try {
        // Fetch user info
        const info = await invoke<{
          ok: boolean; name?: string; ip?: string; seen_ip?: string;
          gb_used?: number; gb_total?: number; days_left?: number;
          speed_mbps?: number; plan_name?: string; plan?: string;
          expires?: string; status?: string;
        }>("get_user_info", { panelUrl, session });

        if (info.ok) {
          setUserInfo(info as never);
          // Relay IP comes from the user's registered IP or seen IP
          const ip = info.ip || info.seen_ip;
          if (ip) setRelayIp(ip);
        } else {
          // Session expired or invalid
          logout();
          setScreen("login");
          return;
        }

        // Fetch plans
        const plansResp = await invoke<{
          ok: boolean; plans?: Array<{
            id: number; name: string; price: number;
            desc?: string; days?: number; gb?: number; mbps?: number;
          }>;
        }>("get_plans", { panelUrl, session });

        if (plansResp.ok && plansResp.plans) {
          setPlans(plansResp.plans);
        }

        // Get current DNS status
        const dns = await invoke<{
          configured: boolean; current_dns?: string; interface: string;
        }>("get_dns_status");
        setDnsStatus(dns as never);

        // Check if already connected (DNS is set to relay)
        if (dns.configured && dns.current_dns) {
          setConnectionStatus("connected");
        }
      } catch (e) {
        setError(String(e));
      }
    };

    fetchData();
  }, [session]);

  const handleLogout = async () => {
    try {
      await invoke("disconnect").catch(() => {});
    } catch {}
    logout();
  };

  return (
    <div className="min-h-screen p-6 max-w-lg mx-auto space-y-5">
      {/* Header */}
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl flex items-center justify-center"
               style={{ background: "linear-gradient(135deg, var(--p), var(--p2))" }}>
            <span className="text-sm font-black" style={{ color: "var(--bg)" }}>PX</span>
          </div>
          <div>
            <h1 className="text-lg font-bold" style={{ color: "var(--p)" }}>PeDitXCDN</h1>
            <p className="text-xs" style={{ color: "var(--muted)" }}>
              {userInfo?.plan_name || "سرویس CDN"}
            </p>
          </div>
        </div>
        <button onClick={handleLogout} className="btn-ghost text-sm">
          خروج
        </button>
      </header>

      {/* Connect Button */}
      <ConnectButton />

      {/* Status */}
      <StatusCard />

      {/* Quota */}
      <QuotaBar />

      {/* Plans */}
      <PlansList />

      {/* Expiry info */}
      {userInfo?.expires && (
        <div className="card">
          <div className="flex items-center justify-between">
            <span className="text-sm" style={{ color: "var(--muted)" }}>تاریخ انقضا</span>
            <span className="text-sm font-medium">{userInfo.expires}</span>
          </div>
        </div>
      )}
    </div>
  );
}
