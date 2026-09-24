import { useAppStore } from "../store";
import { IcLayers } from "./icons";

export function PlansList() {
  const plans = useAppStore((s) => s.plans);
  const userInfo = useAppStore((s) => s.userInfo);

  if (!plans.length) return null;

  return (
    <div className="space-y-2">
      <h3 className="flex items-center gap-1.5 text-xs font-medium"
          style={{ color: "var(--muted)" }}>
        <IcLayers size={13} />پلن‌های موجود
      </h3>
      <div className="space-y-2">
        {plans.map((plan) => {
          const isCurrent = userInfo?.plan === String(plan.id);
          return (
            <div
              key={plan.id}
              className="card p-3"
              style={{
                borderColor: isCurrent ? "var(--p)" : "var(--border)",
                background: isCurrent ? "rgba(33,169,255,0.05)" : "var(--card)",
              }}
            >
              <div className="flex items-center justify-between">
                <div>
                  <div className="text-sm font-medium">{plan.name}</div>
                  <div className="text-xs mt-0.5" style={{ color: "var(--muted)" }}>
                    {plan.days} روز · {plan.gb} GB · {plan.mbps} Mb/s
                  </div>
                </div>
                <div className="text-sm font-bold" style={{ color: "var(--p)" }}>
                  {plan.price.toLocaleString("fa-IR")}
                  <span className="text-[10px] font-normal" style={{ color: "var(--muted)" }}>
                    {" "}تومان
                  </span>
                </div>
              </div>
              {isCurrent && (
                <div className="mt-2 text-[10px] font-medium px-2 py-0.5 rounded inline-block"
                     style={{ background: "rgba(33,169,255,0.15)", color: "var(--p)" }}>
                  پلن فعلی
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
