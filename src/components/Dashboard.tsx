import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import { ConnectButton } from "./ConnectButton";
import { StatusCard } from "./StatusCard";
import { QuotaBar } from "./QuotaBar";
import { PlansList } from "./PlansList";

const PANEL_URL_FALLBACK = "https://docproir.peditxcdn.ir:8443";
const BOT_USERNAME = "PeDitXDNS_bot";

/** Whole calendar days from today until a `YYYY-MM-DD` expiry date. */
function daysFromExpires(expires?: string): number | undefined {
  if (!expires) return undefined;
  const [y, m, d] = expires.split("-").map(Number);
  if (!y || !m || !d) return undefined;
  const end = new Date(y, m - 1, d);
  end.setHours(0, 0, 0, 0);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  return Math.max(0, Math.round((end.getTime() - today.getTime()) / 86_400_000));
}

/**
 * The deployed panel always reports `days_left: 0` — it computes
 * `expires_at - now()` where `now()` returns a string, the TypeError is
 * swallowed by `except: pass`, while `expires` itself comes back as a
 * plain slice and renders fine. Until that ships fixed server-side,
 * recompute here whenever the panel's answer is unusable.
 */
function withDays<T extends { days_left?: number; expires?: string }>(info: T): T {
  if (!info.days_left) {
    const d = daysFromExpires(info.expires);
    if (d) info.days_left = d;
  }
  return info;
}

export function Dashboard() {
  const {
    session, panelUrl, userInfo,
    setUserInfo, setPlans, setRelayIp, setDnsStatus,
    setConnectionStatus, logout, setError, setScreen,
  } = useAppStore();
  const [updatingIp, setUpdatingIp] = useState(false);
  const [ipOk, setIpOk] = useState(false);

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

        if (!info.ok) {
          logout();
          setScreen("login");
          return;
        }

        // Register this machine's address — the relay's nftables ACL keys off it.
        if (info.seen_ip && info.seen_ip !== info.ip) {
          try {
            const claim = await invoke<{ ok: boolean }>("claim_ip", {
              panelUrl, session, ip: info.seen_ip,
            });
            if (claim.ok) info.ip = info.seen_ip;
          } catch { /* non-fatal: panel still reports the old address */ }
        }
        setUserInfo(withDays(info) as never);

        // Relay IP = the panel host. info.ip / info.seen_ip are the user's own
        // address — forwarding DNS to those would blackhole resolution.
        try {
          setRelayIp(await invoke<string>("resolve_relay_ip", { panelUrl }));
        } catch (e) {
          setError(String(e));
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

        if (dns.configured && dns.current_dns === "127.0.0.1") {
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

  // Re-register this machine's address on demand — the relay sees our IP,
  // so a fresh user-info + claim is what actually updates the panel.
  // The relay's ACL is keyed by this address: wrong IP = no network.
  const handleUpdateIp = async () => {
    if (updatingIp) return;
    setUpdatingIp(true);
    setIpOk(false);
    try {
      const info = await invoke<typeof userInfo>("get_user_info", { panelUrl, session });
      if (!info?.ok) {
        setError("دریافت اطلاعات حساب ناموفق بود");
        return;
      }
      if (!info.seen_ip) {
        setError("آی‌پی واقعی شما توسط سرور دیده نشد؛ دوباره تلاش کنید");
        return;
      }
      const claim = await invoke<{ ok: boolean; message?: string }>("claim_ip", {
        panelUrl, session, ip: info.seen_ip,
      });
      if (!claim.ok) {
        setError(claim.message || "ثبت آی‌پی ناموفق بود");
        setUserInfo(withDays(info));
        return;
      }
      info.ip = info.seen_ip;
      setUserInfo(withDays(info));
      setError(null);
      setIpOk(true);
      setTimeout(() => setIpOk(false), 4000);
    } catch (e) {
      setError(String(e));
    } finally {
      setUpdatingIp(false);
    }
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
                  {userInfo.gb_total
                    ? Math.max(0, userInfo.gb_total - (userInfo.gb_used ?? 0)).toFixed(1)
                    : "∞"}
                </div>
                <div className="stat-label">GB باقیمانده</div>
              </div>
            </div>

            {/* Quota Progress */}
            {userInfo.gb_total && (
              <QuotaBar />
            )}

            {/* IP registration — the whole network keys off this being right */}
            <div className="space-y-1.5 pt-1">
              <div className="flex items-center justify-between">
                <span className="text-xs" style={{ color: "var(--muted)" }}>آی‌پی ثبت‌شده در پنل</span>
                <span className="font-mono text-xs"
                      style={{ color: userInfo.ip ? "var(--text)" : "var(--danger)" }}>
                  {userInfo.ip || "ثبت نشده"}
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-xs" style={{ color: "var(--muted)" }}>آی‌پی واقعی شما</span>
                <span className="font-mono text-xs" style={{ color: "var(--text)" }}>
                  {userInfo.seen_ip || "—"}
                </span>
              </div>

              {userInfo.seen_ip && userInfo.seen_ip !== userInfo.ip && (
                <p className="text-[11px] leading-5 p-2 rounded-lg"
                   style={{ color: "var(--warn)", background: "rgba(255,193,7,0.08)",
                            border: "1px solid rgba(255,193,7,0.25)" }}>
                  آی‌پی شما تغییر کرده و هنوز در پنل ثبت نشده؛ تا ثبت نشود شبکه کار نمی‌کند.
                </p>
              )}

              <button onClick={handleUpdateIp} disabled={updatingIp}
                      className="w-full flex items-center justify-center gap-2 mt-1
                                 py-3 rounded-xl text-sm font-bold transition-all"
                      style={{
                        color: "var(--p)",
                        border: "1.5px solid var(--p)",
                        background: updatingIp ? "rgba(0,212,170,0.18)" : "rgba(0,212,170,0.08)",
                        opacity: updatingIp ? 0.75 : 1,
                        boxShadow: ipOk ? "0 0 14px rgba(0,212,170,0.25)" : "none",
                      }}>
                <span className={"inline-block text-base " + (updatingIp ? "animate-spin" : "")}
                      style={{ animationDuration: "0.8s" }}>
                  ⟳
                </span>
                {updatingIp ? "در حال ثبت آی‌پی..." : ipOk ? "آی‌پی ثبت شد ✓" : "به‌روزرسانی آی‌پی"}
              </button>
            </div>

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
          PeDitXCDN v0.3.11
        </span>
      </div>
    </div>
  );
}
