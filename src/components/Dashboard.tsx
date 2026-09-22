import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import { ConnectButton } from "./ConnectButton";
import { StatusCard } from "./StatusCard";
import { QuotaBar } from "./QuotaBar";
import { PlansList } from "./PlansList";

const PANEL_URL_FALLBACK = "https://docproir.peditxcdn.ir:8443";
const BOT_USERNAME = "PeDitXDNS_bot";

export function Dashboard() {
  const {
    session, panelUrl, userInfo,
    setUserInfo, setPlans, setRelayIp, setDnsStatus,
    setConnectionStatus, logout, setError, setScreen,
  } = useAppStore();

  useEffect(() => {
    if (!session) {
      setScreen("login");
      return;
    }

    const fetchData = async () => {
      try {
        const info = await invoke<{
          ok: boolean; name?: string; ip?: string; seen_ip?: string;
          gb_used?: number; gb_total?: number; days_left?: number;
          speed_mbps?: number; plan_name?: string; plan?: string;
          expires?: string; status?: string;
        }>("get_user_info", { panelUrl, session });

        if (info.ok) {
          setUserInfo(info as never);
          const ip = info.ip || info.seen_ip;
          if (ip) setRelayIp(ip);
        } else {
          logout();
          setScreen("login");
          return;
        }

        const plansResp = await invoke<{
          ok: boolean; plans?: Array<{
            id: number; name: string; price: number;
            desc?: string; days?: number; gb?: number; mbps?: number;
          }>;
        }>("get_plans", { panelUrl, session });

        if (plansResp.ok && plansResp.plans) {
          setPlans(plansResp.plans);
        }

        const dns = await invoke<{
          configured: boolean; current_dns?: string; interface: string;
        }>("get_dns_status");
        setDnsStatus(dns as never);

        const relayIp = info.ip || info.seen_ip;
        if (relayIp && dns.configured && dns.current_dns === "127.0.0.1") {
          setConnectionStatus("connected");
        }
      } catch (e) {
        setError(String(e));
      }
    };

    fetchData();
  }, [session]);

  const handleLogout = async () => {
    try { await invoke("disconnect").catch(() => {}); } catch {}
    logout();
  };

  const openPanel = () => {
    window.open(panelUrl || PANEL_URL_FALLBACK, "_blank");
  };

  const openBot = () => {
    window.open(`https://t.me/${BOT_USERNAME}`, "_blank");
  };

  return (
    <div className="h-screen flex flex-col overflow-hidden" style={{ background: "var(--bg)" }}>
      {/* Header */}
      <header className="flex items-center justify-between px-5 py-3 shrink-0"
              style={{ borderBottom: "1px solid var(--border)" }}>
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg overflow-hidden">
            <img src="/logo.png" alt="PeDitXCDN" className="w-full h-full object-cover" />
          </div>
          <div>
            <h1 className="text-sm font-bold" style={{ color: "var(--p)" }}>PeDitXCDN</h1>
            <p className="text-[10px]" style={{ color: "var(--muted)" }}>
              {userInfo?.plan_name || "سرویس CDN"}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <div className={`status-ring ${useAppStore.getState().connectionStatus}`} />
          <button onClick={handleLogout}
                  className="text-xs px-3 py-1.5 rounded-lg transition-colors"
                  style={{ color: "var(--muted)", border: "1px solid var(--border)" }}>
            خروج
          </button>
        </div>
      </header>

      {/* Scrollable Content */}
      <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">

        {/* Connect Button - Large Center */}
        <ConnectButton />

        {/* Account Info Card */}
        {userInfo && (
          <div className="card p-4 space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium" style={{ color: "var(--muted)" }}>
                اطلاعات حساب
              </span>
              <span className="text-xs px-2 py-0.5 rounded-full font-medium"
                    style={{
                      background: userInfo.status === "active"
                        ? "rgba(0,212,170,0.15)" : "rgba(255,71,87,0.15)",
                      color: userInfo.status === "active"
                        ? "var(--success)" : "var(--danger)",
                    }}>
                {userInfo.status === "active" ? "فعال" : "غیرفعال"}
              </span>
            </div>

            {/* Stats Grid */}
            <div className="grid grid-cols-3 gap-2">
              <div className="stat-card">
                <div className="stat-value text-base">
                  {userInfo.days_left ?? "—"}
                </div>
                <div className="stat-label">روز باقیمانده</div>
              </div>
              <div className="stat-card">
                <div className="stat-value text-base">
                  {userInfo.speed_mbps ?? "—"}
                </div>
                <div className="stat-label">سرعت (Mb/s)</div>
              </div>
              <div className="stat-card">
                <div className="stat-value text-base">
                  {userInfo.gb_used?.toFixed(1) ?? "0"}
                </div>
                <div className="stat-label">GB مصرفی</div>
              </div>
            </div>

            {/* Quota Progress */}
            {userInfo.gb_total && (
              <QuotaBar />
            )}

            {/* Expiry */}
            {userInfo.expires && (
              <div className="flex items-center justify-between pt-1">
                <span className="text-xs" style={{ color: "var(--muted)" }}>تاریخ انقضا</span>
                <span className="text-xs font-medium">{userInfo.expires}</span>
              </div>
            )}
          </div>
        )}

        {/* Status */}
        <StatusCard />

        {/* Quick Actions */}
        <div className="grid grid-cols-2 gap-3">
          <button onClick={openPanel} className="card card-hover p-4 text-center space-y-2">
            <div className="text-2xl">💳</div>
            <div className="text-xs font-medium" style={{ color: "var(--text)" }}>شارژ مجدد</div>
            <div className="text-[10px]" style={{ color: "var(--muted)" }}>پنل کاربری</div>
          </button>
          <button onClick={openBot} className="card card-hover p-4 text-center space-y-2">
            <div className="text-2xl">💬</div>
            <div className="text-xs font-medium" style={{ color: "var(--text)" }}>پشتیبانی</div>
            <div className="text-[10px]" style={{ color: "var(--muted)" }}>تلگرام</div>
          </button>
        </div>

        {/* Plans */}
        <PlansList />
      </div>

      {/* Bottom Bar - Branding */}
      <div className="shrink-0 text-center py-2"
           style={{ borderTop: "1px solid var(--border)" }}>
        <span className="text-[10px]" style={{ color: "var(--muted)" }}>
          PeDitXCDN v0.3.3
        </span>
      </div>
    </div>
  );
}
