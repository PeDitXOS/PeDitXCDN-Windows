import { useAppStore } from "../store";

export function QuotaBar() {
  const userInfo = useAppStore((s) => s.userInfo);

  if (!userInfo?.gb_total) return null;

  const used = userInfo.gb_used ?? 0;
  const total = userInfo.gb_total;
  const pct = Math.min(Math.round((used / total) * 100), 100);

  const color = pct > 90 ? "var(--danger)" : pct > 70 ? "var(--warn)" : "var(--p)";

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between">
        <span className="text-[11px]" style={{ color: "var(--muted)" }}>
          حجم باقیمانده: <b style={{ color }}>{Math.max(0, total - used).toFixed(1)} GB</b>
        </span>
        <span className="text-[11px] font-medium">
          {used.toFixed(1)} / {total.toFixed(1)} GB
        </span>
      </div>
      <div className="progress-track">
        <div
          className="progress-fill"
          style={{
            width: `${pct}%`,
            background: `linear-gradient(90deg, ${color}, ${color}cc)`,
          }}
        />
      </div>
    </div>
  );
}
