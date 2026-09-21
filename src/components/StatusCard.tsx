import { useAppStore } from "../store";

export function StatusCard() {
  const { connectionStatus, relayIp, userInfo, error, dnsStatus } = useAppStore();

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

      {dnsStatus?.current_dns && (
        <div className="flex items-center justify-between">
          <span className="text-xs" style={{ color: "var(--muted)" }}>DNS فعلی</span>
          <span className="font-mono text-xs" style={{ color: "var(--text)" }}>
            {dnsStatus.current_dns}
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
