import { useCallback, useEffect, useRef, useState, type UIEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import type { DnsStatus, EmergencyStop, ProxyStatus } from "../types";
import { ConnectButton } from "./ConnectButton";
import { StatusCard } from "./StatusCard";
import { QuotaBar } from "./QuotaBar";
import { PlansList } from "./PlansList";
import {
  IcPower, IcUser, IcActivity, IcLayers,
  IcCalendar, IcGauge, IcDisk, IcRefresh, IcAlert, IcCard, IcChat, IcLogout,
} from "./icons";

const PANEL_URL_FALLBACK = "https://docproir.peditxcdn.ir:8443";
const BOT_USERNAME = "peditxcdn_bot";

/** Rail sections, top to bottom — ids match the scroll targets below. */
const SECTIONS = [
  { id: "sec-connect", title: "اتصال", icon: <IcPower size={18} /> },
  { id: "sec-account", title: "حساب", icon: <IcUser size={18} /> },
  { id: "sec-status", title: "وضعیت", icon: <IcActivity size={18} /> },
  { id: "sec-plans", title: "تعرفه‌ها", icon: <IcLayers size={18} /> },
];

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
 * Fallback for a panel that still reports `days_left: 0` next to a real
 * expiry date (it used to compute `expires_at - now()` where `now()` is a
 * string, and the TypeError was swallowed). Deployed 2026-09-22 on the
 * exit server and switched to calendar days — kept so an unpatched panel
 * degrades to a correct number instead of 0.
 */
function withDays<T extends { days_left?: number; expires?: string }>(info: T): T {
  // Prefer the calendar count when the panel floors at 24h: a plan through
  // Oct 21 must read "29 days" on Sep 22, not 28. The panel switched to
  // calendar days on 2026-09-22; this keeps an older panel correct too.
  const cal = daysFromExpires(info.expires);
  if (cal !== undefined && (!info.days_left || info.days_left < cal)) {
    info.days_left = cal;
  }
  return info;
}

export function Dashboard() {
  const {
    session, panelUrl, userInfo, connectionStatus,
    setUserInfo, setPlans, setRelayIp, setDnsStatus, setProxyStatus,
    setConnectionStatus, logout, setError, setScreen,
  } = useAppStore();
  const [updatingIp, setUpdatingIp] = useState(false);
  const [ipOk, setIpOk] = useState(false);
  // Opt-in only: default off, so opening this app never steals the account's
  // registered address from whatever else the customer is using (max_ips=1).
  const [autoReg, setAutoReg] = useState(
    () => typeof window !== "undefined" &&
          localStorage.getItem("peditx_auto_register") === "1",
  );

  useEffect(() => {
    localStorage.setItem("peditx_auto_register", autoReg ? "1" : "0");
    void invoke("set_auto_register", { on: autoReg });
  }, [autoReg]);

  // Follow the backend's own answer, not our flag: a tray cut, an emergency
  // stop and a dead loop all have to land here, and `connectionStatus` used
  // to keep saying «متصل» after the loops were gone. The read is a Mutex —
  // free at 1 Hz — while netsh (which is not) runs only on a real
  // running→stopped transition.
  const runningRef = useRef<boolean | null>(null);
  useEffect(() => {
    let alive = true;
    const tick = async () => {
      let s: ProxyStatus;
      try {
        s = await invoke<ProxyStatus>("proxy_status");
      } catch {
        return; // backend not answering — keep the last known state
      }
      if (!alive) return;
      setProxyStatus(s);
      const st = useAppStore.getState();
      // Never touch `connecting` (a connect is mid-flight) or `error`
      // (an error the user hasn't seen yet must not be silently cleared).
      if (st.connectionStatus === "connected" || st.connectionStatus === "disconnected") {
        const want = s.running ? "connected" : "disconnected";
        if (st.connectionStatus !== want) setConnectionStatus(want);
      }
      if (runningRef.current !== null && runningRef.current !== s.running) {
        try {
          setDnsStatus(await invoke<DnsStatus>("get_dns_status"));
        } catch { /* keep the last reading */ }
      }
      runningRef.current = s.running;
    };
    tick();
    const id = setInterval(tick, 1000);
    return () => { alive = false; clearInterval(id); };
  }, []);

  // One-key emergency cut: DHCP restore on both stacks, proxy loops stopped,
  // cache flushed — regardless of what either side believes. Reachable from
  // this button, Ctrl+Shift+X and the tray item (which calls Rust directly,
  // so it still works with the webview hung).
  const [emg, setEmg] = useState<"idle" | "busy" | "ok" | "fail" | "unknown">("idle");
  const emgBusy = useRef(false);
  const handleEmergency = useCallback(async () => {
    if (emgBusy.current) return;
    emgBusy.current = true;
    setEmg("busy");
    setError(null);
    try {
      const r = await invoke<EmergencyStop>("emergency_stop");
      setConnectionStatus("disconnected");
      setDnsStatus(await invoke<DnsStatus>("get_dns_status"));
      if (r.dns_restored) {
        setEmg("ok");
        setTimeout(() => setEmg("idle"), 4000);
      } else if (r.current_dns != null || r.ipv6_dns != null) {
        // Do not report success for a restore that did not happen: a failed
        // netsh leaves 127.0.0.1 pointing at a proxy that no longer exists.
        setEmg("fail");
        setError("پروکسی قطع شد اما DNS سیستم هنوز روی ۱۲۷.۰.۰.۱ است؛ لطفاً دستی اصلاح کنید.");
      } else {
        // Addresses came back null — the system could not be read, so the
        // result is unknown, not fixed. Different warning, same caution.
        setEmg("unknown");
        setError("پروکسی قطع شد اما وضعیت DNS خوانده نشد؛ لطفاً دستی بررسی کنید.");
      }
    } catch (e) {
      // The command itself failed — nothing was verified, so say that.
      setEmg("unknown");
      setError(String(e));
    } finally {
      emgBusy.current = false;
    }
  }, [setConnectionStatus, setDnsStatus, setError]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.code === "KeyX") {
        e.preventDefault();
        void handleEmergency();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [handleEmergency]);

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

        // Register this machine's address — the relay's nftables ACL keys off
        // it. Only when the customer ticked the box: an uninvited claim here
        // is exactly what locks their other device out (max_ips=1).
        if (
          localStorage.getItem("peditx_auto_register") === "1" &&
          info.seen_ip && info.seen_ip !== info.ip
        ) {
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

  // window.open is inert in the Tauri webview — both quick-action buttons
  // did nothing until they went through the opener plugin.
  const openExternal = async (url: string) => {
    try {
      await invoke("plugin:opener|open_url", { url });
    } catch (e) {
      setError(String(e));
    }
  };

  const openPanel = () => {
    void openExternal(panelUrl || PANEL_URL_FALLBACK);
  };

  const openBot = () => {
    void openExternal(`https://t.me/${BOT_USERNAME}`);
  };

  // Left rail follows the scroll position; clicking scrolls to that section.
  const [activeSec, setActiveSec] = useState(SECTIONS[0].id);
  const onScroll = (e: UIEvent<HTMLDivElement>) => {
    // Anchor just under the card's own top edge — same line every section
    // header passes on its way up.
    const limit = e.currentTarget.getBoundingClientRect().top + 140;
    let cur = SECTIONS[0].id;
    for (const s of SECTIONS) {
      const el = document.getElementById(s.id);
      if (el && el.getBoundingClientRect().top <= limit) cur = s.id;
    }
    setActiveSec(cur);
  };
  const goSection = (id: string) => {
    setActiveSec(id);
    document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  return (
    <div className="h-screen flex flex-col overflow-hidden">
      {/* Header — 420px window: the brand block is the only thing allowed to
          shrink, so the emergency cut never wraps to a second line. */}
      <header className="flex items-center justify-between gap-2 px-4 py-3 shrink-0"
              style={{ borderBottom: "1px solid var(--border)" }}>
        <div className="flex items-center gap-3 min-w-0">
          <div className="w-8 h-8 rounded-full overflow-hidden shrink-0"
               style={{ boxShadow: "0 0 0 1px rgba(33,169,255,0.35)" }}>
            <img src="/logo.png" alt="PeDitXCDN" className="w-full h-full object-cover" />
          </div>
          <div className="min-w-0">
            <h1 className="text-sm font-bold truncate" style={{ color: "var(--p)" }}>PeDitXCDN</h1>
            <p className="text-[10px] truncate" style={{
                  color: "var(--muted)", fontFamily: "var(--mono)", letterSpacing: "0.08em",
                }}>
              gaming · {userInfo?.plan_name || "سرویس CDN"}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <div className={`status-ring ${connectionStatus}`} />
          {/* Always present: this is the button pressed when something is
              already wrong, so it must never be hidden behind a state. */}
          <button onClick={() => void handleEmergency()}
                  disabled={emg === "busy"}
                  title="قطع همه‌چیز: توقف پروکسی و بازگشت DNS سیستم به DHCP (Ctrl+Shift+X)"
                  className="flex items-center gap-1.5 whitespace-nowrap text-xs px-2.5 py-1.5 rounded transition-colors"
                  style={{
                    color: emg === "ok" ? "var(--success)"
                        : emg === "fail" || emg === "unknown" ? "var(--warn)"
                        : "var(--danger)",
                    border: `1px solid ${
                      emg === "ok" ? "rgba(33,169,255,0.45)"
                      : emg === "fail" || emg === "unknown" ? "rgba(255,193,7,0.45)"
                      : "rgba(255,71,87,0.45)"}`,
                    background: emg === "ok" ? "rgba(33,169,255,0.12)"
                        : emg === "fail" || emg === "unknown" ? "rgba(255,193,7,0.1)"
                        : "transparent",
                    opacity: emg === "busy" ? 0.6 : 1,
                  }}>
            <IcAlert size={13} />
            {emg === "busy" ? "در حال قطع…"
              : emg === "ok" ? "قطع شد ✓"
              : emg === "fail" ? "DNS اصلاح نشد"
              : emg === "unknown" ? "DNS بررسی نشد"
              : "قطع اضطراری"}
          </button>
        </div>
      </header>

      <div className="flex-1 flex overflow-hidden">
        {/* Left icon rail — the one structure every gaming dashboard shares */}
        <nav className="rail">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              type="button"
              title={s.title}
              aria-label={s.title}
              aria-current={activeSec === s.id ? "true" : undefined}
              className={`rail-item ${activeSec === s.id ? "active" : ""}`}
              onClick={() => goSection(s.id)}
            >
              {s.icon}
            </button>
          ))}
          {/* Sign-out lives at the foot of the rail: a labelled button in the
              header is what forced the brand block down to 30px at 420px. */}
          <button
            type="button"
            title="خروج"
            aria-label="خروج"
            className="rail-item mt-auto"
            onClick={handleLogout}
          >
            <IcLogout size={18} />
          </button>
        </nav>

        {/* Scrollable Content */}
        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4" onScroll={onScroll}>

        {/* Connect Button - Large Center */}
        <div id="sec-connect" className="scroll-mt-2">
          <ConnectButton />
        </div>

        {/* Account Info Card */}
        {userInfo && (
          <div id="sec-account" className="card p-4 space-y-3 scroll-mt-2">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium" style={{ color: "var(--muted)" }}>
                اطلاعات حساب
              </span>
              <span className="text-xs px-2 py-0.5 rounded font-medium"
                    style={{
                      background: userInfo.status === "active"
                        ? "rgba(33,169,255,0.15)" : "rgba(255,71,87,0.15)",
                      color: userInfo.status === "active"
                        ? "var(--success)" : "var(--danger)",
                    }}>
                {userInfo.status === "active" ? "فعال" : "غیرفعال"}
              </span>
            </div>

            {/* Stats Grid */}
            <div className="grid grid-cols-3 gap-2">
              <div className="stat-card">
                <div className="flex justify-center mb-1" style={{ color: "var(--p)" }}>
                  <IcCalendar size={15} />
                </div>
                <div className="stat-value text-base">
                  {userInfo.days_left ?? "—"}
                </div>
                <div className="stat-label">روز باقیمانده</div>
              </div>
              <div className="stat-card">
                <div className="flex justify-center mb-1" style={{ color: "var(--p)" }}>
                  <IcGauge size={15} />
                </div>
                <div className="stat-value text-base">
                  {userInfo.speed_mbps ?? "—"}
                </div>
                <div className="stat-label">سرعت (Mb/s)</div>
              </div>
              <div className="stat-card">
                <div className="flex justify-center mb-1" style={{ color: "var(--p)" }}>
                  <IcDisk size={15} />
                </div>
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
              <div className="row">
                <span className="k">آی‌پی ثبت‌شده در پنل</span>
                <span className="v font-mono"
                      style={{ color: userInfo.ip ? "var(--text)" : "var(--danger)" }}>
                  {userInfo.ip || "ثبت نشده"}
                </span>
              </div>
              <div className="row">
                <span className="k">آی‌پی واقعی شما</span>
                <span className="v font-mono">{userInfo.seen_ip || "—"}</span>
              </div>

              {userInfo.seen_ip && userInfo.seen_ip !== userInfo.ip && (
                <p className="text-xs leading-5 p-2 rounded"
                   style={{ color: "var(--warn)", background: "rgba(255,193,7,0.08)",
                            border: "1px solid rgba(255,193,7,0.25)" }}>
                  آی‌پی شما تغییر کرده و هنوز در پنل ثبت نشده؛ تا ثبت نشود شبکه کار نمی‌کند.
                </p>
              )}

              <button onClick={handleUpdateIp} disabled={updatingIp}
                      className="w-full flex items-center justify-center gap-2 mt-1
                                 py-3 rounded text-sm font-bold transition-all"
                      style={{
                        color: "var(--p)",
                        border: "1.5px solid var(--p)",
                        background: updatingIp ? "rgba(33,169,255,0.18)" : "rgba(33,169,255,0.08)",
                        opacity: updatingIp ? 0.75 : 1,
                        boxShadow: ipOk ? "0 0 14px rgba(33,169,255,0.25)" : "none",
                      }}>
                <IcRefresh size={14} />
                {updatingIp ? "در حال ثبت آی‌پی..." : ipOk ? "آی‌پی ثبت شد ✓" : "به‌روزرسانی آی‌پی"}
              </button>

              <label className="flex items-start gap-2 pt-1.5 cursor-pointer select-none">
                <input
                  type="checkbox"
                  checked={autoReg}
                  onChange={(e) => setAutoReg(e.target.checked)}
                  className="mt-0.5 w-3.5 h-3.5"
                />
                <span className="text-xs" style={{ color: "var(--muted)" }}>
                  ثبت خودکار آی‌پی هنگام اتصال
                  <span className="block text-[10px] leading-4 opacity-80">
                    برای استفاده از حساب روی دستگاه دیگر، خاموش بماند؛ فقط با تیک شما فعال می‌شود
                  </span>
                </span>
              </label>
            </div>

            {/* Expiry */}
            {userInfo.expires && (
              <div className="row pt-1">
                <span className="k">تاریخ انقضا</span>
                <span className="v font-medium">{userInfo.expires}</span>
              </div>
            )}
          </div>
        )}

        {/* Status */}
        <div id="sec-status" className="scroll-mt-2">
          <StatusCard />
        </div>

        {/* Quick Actions */}
        <div className="grid grid-cols-2 gap-3">
          <button onClick={openPanel} className="card card-hover p-3.5 text-center space-y-1">
            <div className="flex justify-center" style={{ color: "var(--p)" }}>
              <IcCard size={18} />
            </div>
            <div className="text-xs font-medium" style={{ color: "var(--text)" }}>شارژ مجدد</div>
            <div className="text-[10px]" style={{ color: "var(--muted)" }}>پنل کاربری</div>
          </button>
          <button onClick={openBot} className="card card-hover p-3.5 text-center space-y-1">
            <div className="flex justify-center" style={{ color: "var(--p)" }}>
              <IcChat size={18} />
            </div>
            <div className="text-xs font-medium" style={{ color: "var(--text)" }}>پشتیبانی</div>
            <div className="text-[10px]" style={{ color: "var(--muted)" }}>تلگرام</div>
          </button>
        </div>

        {/* Plans */}
        <div id="sec-plans" className="scroll-mt-2">
          <PlansList />
        </div>
        </div>
      </div>

      {/* Bottom Bar - Branding */}
      <div className="shrink-0 text-center py-2"
           style={{ borderTop: "1px solid var(--border)" }}>
        <span className="text-[10px]" style={{
                color: "var(--muted)", fontFamily: "var(--mono)", letterSpacing: "0.12em",
              }}>
          PeDitX© v0.3.23
        </span>
      </div>
    </div>
  );
}
