import { useAppStore } from "../store";

export function StatusCard() {
  const { status, relay_ip, panel_data, error } = useAppStore();

  const statusLabel = {
    disconnected: "قطع شده",
    connecting: "در حال اتصال",
    connected: "متصل",
    error: "خطا",
  }[status];

  const statusColor = {
    disconnected: "text-gray-400",
    connecting: "text-yellow-400",
    connected: "text-brand-400",
    error: "text-red-400",
  }[status];

  const quotaPct =
    panel_data && panel_data.quota_total > 0
      ? Math.round((panel_data.quota_used / panel_data.quota_total) * 100)
      : 0;

  return (
    <div className="bg-gray-900 rounded-2xl p-5 space-y-3 border border-gray-800">
      <div className="flex items-center justify-between">
        <span className="text-sm text-gray-400">وضعیت</span>
        <span className={statusColor}>{statusLabel}</span>
      </div>

      {relay_ip && (
        <div className="flex items-center justify-between">
          <span className="text-sm text-gray-400">آی‌پی رله</span>
          <span className="font-mono text-sm">{relay_ip}</span>
        </div>
      )}

      {panel_data && (
        <>
          <div className="flex items-center justify-between">
            <span className="text-sm text-gray-400">ساب‌دامین</span>
            <span className="text-sm">{panel_data.sub_domain}</span>
          </div>
          <div className="space-y-1">
            <div className="flex items-center justify-between text-sm">
              <span className="text-gray-400">حجم مصرفی</span>
              <span>
                {panel_data.quota_used} / {panel_data.quota_total} MB
              </span>
            </div>
            <div className="w-full bg-gray-800 rounded-full h-2">
              <div
                className="bg-brand-500 h-2 rounded-full transition-all"
                style={{ width: `${quotaPct}%` }}
              />
            </div>
          </div>
        </>
      )}

      {error && (
        <div className="text-red-400 text-sm bg-red-900/20 rounded-lg p-2">
          {error}
        </div>
      )}
    </div>
  );
}
