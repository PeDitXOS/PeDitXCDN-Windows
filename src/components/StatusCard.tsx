import { useAppStore } from "../store";

export function StatusCard() {
  const { connectionStatus, relayIp, userInfo, error, dnsStatus } = useAppStore();

  const statusLabel: Record<string, string> = {
    disconnected: "قطع شده",
    connecting: "در حال اتصال",
    connected: "متصل",
    error: "خطا",
  };

  const statusColor: Record<string, string> = {
    disconnected: "var(--muted)",
    connecting: "var(--warn)",
    connected: "var(--success)",
    error: "var(--danger)",
  };

  return (
    <div className="card space-y-3">
      {/* Connection Status */}
      <div className="flex items-center justify-between">
        <span className="text-sm" style={{ color: "var(--muted)" }}>وضعیت اتصال</span>
        <div className="flex items-center gap-2">
          <span className={`status-dot ${connectionStatus}`} />
          <span className="text-sm font-medium" style={{ color: statusColor[connectionStatus] }}>
            {statusLabel[connectionStatus]}
          </span>
        </div>
      </div>

      {/* Relay IP */}
      {relayIp && (
        <div className="flex items-center justify-between">
          <span className="text-sm" style={{ color: "var(--muted)" }}>آی‌پی رله</span>
          <span className="font-mono text-sm" style={{ color: "var(--text)" }}>{relayIp}</span>
        </div>
      )}

      {/* Current DNS */}
      {dnsStatus?.current_dns && (
        <div className="flex items-center justify-between">
          <span className="text-sm" style={{ color: "var(--muted)" }}>DNS فعلی</span>
          <span className="font-mono text-sm" style={{ color: "var(--text)" }}>{dnsStatus.current_dns}</span>
        </div>
      )}

      {/* User Name */}
      {userInfo?.name && (
        <div className="flex items-center justify-between">
          <span className="text-sm" style={{ color: "var(--muted)" }}>کاربر</span>
          <span className="text-sm" style={{ color: "var(--text)" }}>{userInfo.name}</span>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="text-sm p-3 rounded-xl" style={{
          color: "var(--danger)",
          background: "rgba(255,80,80,0.1)",
          border: "1px solid rgba(255,80,80,0.2)",
        }}>
          {error}
        </div>
      )}
    </div>
  );
}
