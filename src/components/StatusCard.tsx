import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import { IcDown, IcUp } from "./icons";

const fmtRate = (bps: number) =>
  bps >= 1e6 ? `${(bps / 1e6).toFixed(1)} MB/s`
  : bps >= 1e3 ? `${(bps / 1e3).toFixed(0)} KB/s`
  : `${Math.round(bps)} B/s`;

const fmtUptime = (s: number) =>
  s < 60 ? `${s} ثانیه`
  : s < 3600 ? `${Math.floor(s / 60)} دقیقه`
  : `${Math.floor(s / 3600)} ساعت ${Math.floor((s % 3600) / 60)} دقیقه`;

export function StatusCard() {
  const { connectionStatus, relayIp, userInfo, error, dnsStatus, proxyStatus } = useAppStore();
  const [rate, setRate] = useState<{ recv: number; sent: number } | null>(null);
  const [probe, setProbe] = useState<
    | { kind: "ok"; a: string; viaRelay: boolean; aaaa: string; ms: number }
    | { kind: "err"; text: string }
    | null
  >(null);

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
  // A = the record the relay hijacks; a live AAAA would let the browser
  // leave the tunnel over IPv6, so both are shown separately.
  useEffect(() => {
    if (connectionStatus !== "connected") return;
    let alive = true;
    const probeDns = async () => {
      try {
        const r = await invoke<{ a: string[]; aaaa: string[]; ms: number }>(
          // relayIp lets the backend split "our proxy is down" from "the
          // network eats port 53" — otherwise both read as a bare timeout.
          "resolve_local", { domain: "youtube.com", relayIp },
        );
        if (!alive) return;
        setProbe({
          kind: "ok",
          a: r.a[0] ?? "—",
          viaRelay: !!relayIp && r.a.some((i) => i === relayIp),
          aaaa: r.aaaa[0] ?? "",
          ms: r.ms,
        });
      } catch (e) {
        if (alive) setProbe({ kind: "err", text: String(e) });
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
             ? "0 0 20px rgba(33,169,255,0.1)"
             : "none",
         }}>
      <div className="row">
        <span className="k">وضعیت اتصال</span>
        <div className="flex items-center gap-2">
          <div className="status-ring" style={{
            background: statusColor[connectionStatus],
            boxShadow: `0 0 8px ${statusColor[connectionStatus]}`,
          }} />
          <span className="font-medium" style={{ color: statusColor[connectionStatus] }}>
            {statusLabel[connectionStatus]}
          </span>
        </div>
      </div>

      {relayIp && (
        <div className="row">
          <span className="k">آی‌پی رله</span>
          <span className="v font-mono">{relayIp}</span>
        </div>
      )}

      {/* Straight from the running loops: proof the backend agrees we are
          up, and how long it has been — not a flag the UI set itself. */}
      {proxyStatus?.running && (
        <div className="row">
          <span className="k">مدت فعالیت پروکسی</span>
          <span className="v font-mono">
            {fmtUptime(proxyStatus.uptime_secs)}
            {proxyStatus.v6 ? "  · دو پشته" : ""}
          </span>
        </div>
      )}

      {rate && (
        <div className="grid grid-cols-2 gap-2 pt-1">
          <div className="stat-card">
            <div className="flex items-center justify-center gap-1.5" style={{ color: "var(--success)" }}>
              <IcDown size={14} />
              <span className="stat-value text-sm" style={{ color: "var(--success)" }}>
                {fmtRate(rate.recv)}
              </span>
            </div>
            <div className="stat-label">دانلود</div>
          </div>
          <div className="stat-card">
            <div className="flex items-center justify-center gap-1.5" style={{ color: "var(--p)" }}>
              <IcUp size={14} />
              <span className="stat-value text-sm" style={{ color: "var(--p)" }}>
                {fmtRate(rate.sent)}
              </span>
            </div>
            <div className="stat-label">آپلود</div>
          </div>
        </div>
      )}

      {dnsStatus?.current_dns && (
        <div className="row">
          <span className="k">DNS فعلی</span>
          <span className="v font-mono">{dnsStatus.current_dns}</span>
        </div>
      )}

      {dnsStatus?.ipv6_dns && (
        <div className="row">
          <span className="k">DNS شش‌خانه</span>
          <span className="v font-mono" style={{
            color: dnsStatus.ipv6_dns === "::1" ? "var(--success)" : "var(--warn)",
          }}>
            {dnsStatus.ipv6_dns}{dnsStatus.ipv6_dns === "::1" ? "  ✓" : "  ⚠"}
          </span>
        </div>
      )}

      {connectionStatus === "connected" && (
        <div className="space-y-1 pt-1">
          <div className="row">
            <span className="k">تست DNS محلی (A)</span>
            <span className="v font-mono" style={{
              color: !probe ? "var(--muted)"
                : probe.kind === "err" ? "var(--danger)"
                : probe.viaRelay ? "var(--success)" : "var(--warn)",
            }}>
              {!probe
                ? "در حال بررسی..."
                : probe.kind === "err"
                ? probe.text
                : `youtube.com → ${probe.a}${probe.viaRelay ? "  ✓ از رله" : "  ⚠ مستقیم"} (${probe.ms}ms)`}
            </span>
          </div>
          {probe?.kind === "ok" && (
            <div className="row">
              <span className="k">IPv6 (AAAA)</span>
              <span className="v font-mono" style={{
                color: probe.aaaa ? "var(--warn)" : "var(--success)",
              }}>
                {probe.aaaa
                  ? `${probe.aaaa}  ⚠ مستقیم (از تونل خارج می‌شود)`
                  : "مسدود ✓  (اجبار IPv4)"}
              </span>
            </div>
          )}
        </div>
      )}

      {userInfo?.name && (
        <div className="row">
          <span className="k">کاربر</span>
          <span className="v">{userInfo.name}</span>
        </div>
      )}

      {error && (
        <div className="text-xs p-2 rounded" style={{
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
