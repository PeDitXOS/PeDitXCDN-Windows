import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";

export function ConnectButton() {
  const {
    connectionStatus, relayIp, setConnectionStatus,
    setRelayIp, setDnsStatus, setError,
  } = useAppStore();
  const loading = connectionStatus === "connecting";

  const toggle = async () => {
    if (connectionStatus === "connected") {
      try {
        await invoke("disconnect");
        setConnectionStatus("disconnected");
        setRelayIp(null);
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

  return (
    <button
      onClick={toggle}
      disabled={loading || !relayIp}
      className="w-full text-lg"
      style={{
        background: loading
          ? "var(--border)"
          : connectionStatus === "connected"
          ? "var(--danger)"
          : "linear-gradient(135deg, var(--p), var(--p2))",
        color: "var(--bg)",
        fontWeight: 700,
        padding: "16px",
        borderRadius: "16px",
        opacity: !relayIp ? 0.4 : 1,
        cursor: !relayIp ? "not-allowed" : "pointer",
        transition: "all 0.2s",
      }}
    >
      {loading
        ? "در حال اتصال..."
        : connectionStatus === "connected"
        ? "قطع اتصال"
        : "اتصال"}
    </button>
  );
}
