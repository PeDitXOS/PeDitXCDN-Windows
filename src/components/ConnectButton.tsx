import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";

export function ConnectButton() {
  const { connected, status, setStatus, setConnected, setRelayIp, setError } =
    useAppStore();
  const loading = status === "connecting";

  const toggle = async () => {
    if (connected) {
      try {
        await invoke("disconnect");
        setConnected(false);
        setStatus("disconnected");
        setRelayIp(null);
      } catch (e) {
        setError(String(e));
      }
      return;
    }

    setStatus("connecting");
    setError(null);
    try {
      const ip = await invoke<string>("connect");
      setConnected(true);
      setStatus("connected");
      setRelayIp(ip);
    } catch (e) {
      setConnected(false);
      setStatus("error");
      setError(String(e));
    }
  };

  return (
    <button
      onClick={toggle}
      disabled={loading}
      className={
        "w-full py-4 rounded-2xl text-lg font-bold transition-all duration-200 " +
        (loading
          ? "bg-brand-700 cursor-wait animate-pulse"
          : connected
            ? "bg-red-600 hover:bg-red-500"
            : "bg-brand-600 hover:bg-brand-500")
      }
    >
      {loading ? "در حال اتصال..." : connected ? "قطع اتصال" : "اتصال"}
    </button>
  );
}
