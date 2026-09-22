import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";

const fmtRate = (bps: number) =>
  bps >= 1e6 ? `${(bps / 1e6).toFixed(1)} MB/s`
  : bps >= 1e3 ? `${(bps / 1e3).toFixed(0)} KB/s`
  : `${Math.round(bps)} B/s`;

export function StatusCard() {
  const { connectionStatus, relayIp, userInfo, error, dnsStatus } = useAppStore();
  const [rate, setRate] = useState<{ recv: number; sent: number } | null>(null);
  const [probe, setProbe] = useState<{ ok: boolean; text: string; viaRelay: boolean } | null>(null);

  useEffect(() => {
    if (connectionStatus !== "connected") {
      setRate(null);
      setProbe(null);
      return;
    }
    let alive = true;
    const tick = async () => {
      try {
        const r = await invoke<[number, number]>("get_net_speed");
        if (alive) setRate({ recv: r[0], sent: r[1] });
      } catch { /* keep the last sample */ }
    };
    tick();
    const id = setInterval(tick, 1000);
    return () => { alive = false; clearInterval(id); };
  }, [connectionStatus]);

  // Resolve through 127.0.0.1:53 itself — the one thing that proves the
  // proxy answers instead of just a status label saying "connected".
  useEffect(() => {
    if (connectionStatus !== "connected") return;
    let alive = true;
    const probeDns = async () => {
      try {
        const ips = await invoke<string[]>("resolve_local", { domain: "youtube.com" });
        if (!alive) return;
        const first = ips[0] ?? "";
        setProbe({
          ok: true,
          text: first,
          viaRelay: !!relayIp && ips.some((i) => i === relayIp),
        });
      } catch (e) {
        if (alive) setProbe({ ok: false, text: String(e), viaRelay: false });
      }
    };
    probeDns();
    const id = setInterval(probeDns, 10_000);
    return () => { alive = false; clearInterval(id); };
  }, [connectionStatus, relayIp]);

  if (!connectionStatus || connectionStatus === "disconnected") return null;

  const statusLabel: Record<string, string> = {
    connecting: "در حال اتصال",
    connected: "متصل",
    error: "خطا",
  };

  const statusColor: Record<string, string> = {
    connecting: "var(--warn)",
    connected: "var(--success)",
    error: "var(--danger)",
  };

  return (
    <div className="card p-4 space-y-2"
         style={{
           borderColor: statusColor[connectionStatus] || "var(--border)",
           boxShadow: connectionStatus === "connected"
             ? "0 0 20px rgba(0,212,170,0.1)"
             : "none",
         }}>
      <div className="flex items-center justify-between">
        <span className="text-xs" style={{ color: "var(--muted)" }}>وضعیت اتصال</span>
        <div className="flex items-center gap-2">
          <div className="status-ring" style={{
            background: statusColor[connectionStatus],
            boxShadow: `0 0 8px ${statusColor[connectionStatus]}`,
          }} />
          <span className="text-xs font-medium" style={{ color: statusColor[connectionStatus] }}>
            {statusLabel[connectionStatus]}
          </span>
        </div>
      </div>

      {relayIp && (
        <div className="flex items-center justify-between">
          <span className="text-xs" style={{ color: "var(--muted)" }}>آی‌پی رله</span>
          <span className="font-mono text-xs" style={{ color: "var(--text)" }}>{relayIp}</span>
        </div>
      )}

      {rate && (
        <div className="grid grid-cols-2 gap-2 pt-1">
          <div className="stat-card">
            <div className="stat-value text-sm" style={{ color: "var(--success)" }}>
              ↓ {fmtRate(rate.recv)}
            </div>
            <div className="stat-label">دانلود</div>
          </div>
          <div className="stat-card">
            <div className="stat-value text-sm" style={{ color: "var(--p)" }}>
              ↑ {fmtRate(rate.sent)}
            </div>
            <div className="stat-label">آپلود</div>
          </div>
        </div>
      )}

      {dnsStatus?.current_dns && (
        <div className="flex items-center justify-between">
          <span className="text-xs" style={{ color: "var(--muted)" }}>DNS فعلی</span>
          <span className="font-mono text-xs" style={{ color: "var(--text)" }}>
            {dnsStatus.current_dns}
          </span>
        </div>
      )}

      {connectionStatus === "connected" && (
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs shrink-0" style={{ color: "var(--muted)" }}>
            تست پروکسی محلی
          </span>
          <span className="font-mono text-xs truncate" style={{
            color: !probe ? "var(--muted)"
              : probe.ok ? (probe.viaRelay ? "var(--success)" : "var(--warn)")
              : "var(--danger)",
          }}>
            {!probe
              ? "در حال بررسی..."
              : probe.ok
              ? `youtube.com → ${probe.text}${probe.viaRelay ? "  ✓ از رله" : "  مستقیم"}`
              : probe.text}
          </span>
        </div>
      )}

      {userInfo?.name && (
        <div className="flex items-center justify-between">
          <span className="text-xs" style={{ color: "var(--muted)" }}>کاربر</span>
          <span className="text-xs" style={{ color: "var(--text)" }}>{userInfo.name}</span>
        </div>
      )}

      {error && (
        <div className="text-xs p-2 rounded-lg" style={{
          color: "var(--danger)",
          background: "rgba(255,71,87,0.1)",
          border: "1px solid rgba(255,71,87,0.2)",
        }}>
          {error}
        </div>
      )}
    </div>
  );
}
