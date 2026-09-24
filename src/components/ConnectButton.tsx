import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import { IcPower } from "./icons";

export function ConnectButton() {
  const {
    connectionStatus, relayIp, setConnectionStatus,
    setDnsStatus, setError,
  } = useAppStore();
  const loading = connectionStatus === "connecting";

  const toggle = async () => {
    if (connectionStatus === "connected") {
      try {
        await invoke("disconnect");
        setConnectionStatus("disconnected");
        // Keep relayIp: it is the resolved panel host and is what the next
        // connect needs. Clearing it here made the Connect button a no-op
        // ("آی‌پی رله در دسترسی نیست") for every reconnect.
        const dns = await invoke<{ configured: boolean; current_dns?: string }>("get_dns_status");
        setDnsStatus(dns as never);
      } catch (e) {
        setError(String(e));
      }
      return;
    }

    if (!relayIp) {
      setError("آی‌پی رله در دسترسی نیست. ابتدا وارد شوید.");
      return;
    }

    setConnectionStatus("connecting");
    setError(null);
    try {
      await invoke<string>("connect", { relayIp });
      setConnectionStatus("connected");
      const dns = await invoke<{ configured: boolean; current_dns?: string }>("get_dns_status");
      setDnsStatus(dns as never);
    } catch (e) {
      setConnectionStatus("error");
      setError(String(e));
    }
  };

  const stateClass = loading
    ? "connecting"
    : connectionStatus === "connected"
    ? "connected"
    : connectionStatus === "error"
    ? "error"
    : "disconnected";

  const label = loading
    ? "در حال اتصال..."
    : connectionStatus === "connected"
    ? "قطع اتصال"
    : "اتصال";

  const sublabel = loading
    ? "پیکربندی DNS..."
    : connectionStatus === "connected"
    ? "DNS فعال است"
    : relayIp
    ? `هدف: ${relayIp}`
    : "آماده اتصال";

  return (
    <button onClick={toggle} disabled={loading || !relayIp}
            className={`connect-btn ${stateClass}`}>
      {/* Pulse animation when connected */}
      {connectionStatus === "connected" && (
        <div className="absolute inset-0 rounded glow-success" />
      )}

      <div className="relative z-10 flex flex-col items-center gap-1.5">
        <span className="opacity-90">
          <IcPower size={26} />
        </span>

        {/* Main Label */}
        <div className="text-lg font-bold">{label}</div>

        {/* Sub Label */}
        <div className="text-xs mt-0.5 opacity-70">{sublabel}</div>
      </div>
    </button>
  );
}
