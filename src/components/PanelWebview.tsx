export function PanelWebview() {
  return (
    <div className="bg-gray-900 rounded-2xl overflow-hidden border border-gray-800">
      <div className="px-4 py-2 bg-gray-800 flex items-center gap-2">
        <span className="w-3 h-3 rounded-full bg-red-500" />
        <span className="w-3 h-3 rounded-full bg-yellow-500" />
        <span className="w-3 h-3 rounded-full bg-green-500" />
        <span className="text-xs text-gray-400 mr-auto">پنل مدیریت</span>
      </div>
      <iframe
        src="https://docproir.peditxcdn.ir:8443"
        className="w-full border-0 bg-white"
        style={{ height: "400px" }}
        title="پنل مدیریت CDN"
        sandbox="allow-same-origin allow-scripts allow-popups"
      />
    </div>
  );
}
