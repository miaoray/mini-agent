import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type DebugLogEntry = {
  id: string;
  turnId: string;
  requestJson: string;
  responseJson: string | null;
  createdAt: number;
};

/**
 * Debug panel component for toggling debug mode and viewing logs
 */
export default function DebugPanel() {
  const [debugMode, setDebugMode] = useState(false);
  const [logs, setLogs] = useState<DebugLogEntry[]>([]);
  const [showLogs, setShowLogs] = useState(false);
  const [selectedLog, setSelectedLog] = useState<DebugLogEntry | null>(null);

  useEffect(() => {
    void loadDebugMode();
  }, []);

  async function loadDebugMode() {
    try {
      const enabled = await invoke<boolean>("get_debug_mode");
      setDebugMode(enabled);
    } catch (error) {
      console.error("[debug-panel] Failed to load debug mode:", error);
    }
  }

  async function toggleDebugMode() {
    try {
      const newMode = !debugMode;
      await invoke("set_debug_mode", { enabled: newMode });
      setDebugMode(newMode);
    } catch (error) {
      console.error("[debug-panel] Failed to set debug mode:", error);
    }
  }

  async function loadLogs() {
    try {
      const entries = await invoke<DebugLogEntry[]>("list_debug_logs", { limit: 10 });
      setLogs(entries);
    } catch (error) {
      console.error("[debug-panel] Failed to load logs:", error);
    }
  }

  function formatJson(json: string): string {
    try {
      return JSON.stringify(JSON.parse(json), null, 2);
    } catch {
      return json;
    }
  }

  return (
    <div className="debug-panel">
      <div className="debug-panel-header">
        <label className="debug-toggle">
          <input
            type="checkbox"
            checked={debugMode}
            onChange={() => void toggleDebugMode()}
          />
          <span>Debug Mode</span>
        </label>
        {debugMode && (
          <button
            type="button"
            className="debug-logs-btn"
            onClick={() => {
              setShowLogs(!showLogs);
              if (!showLogs) {
                void loadLogs();
              }
            }}
          >
            {showLogs ? "Hide Logs" : "Show Logs"}
          </button>
        )}
      </div>

      {showLogs && debugMode && (
        <div className="debug-logs">
          {logs.length === 0 ? (
            <p className="debug-logs-empty">No logs yet.</p>
          ) : (
            <>
              <div className="debug-logs-list">
                {logs.map((log) => (
                  <button
                    key={log.id}
                    type="button"
                    className={`debug-log-item ${selectedLog?.id === log.id ? "active" : ""}`}
                    onClick={() => setSelectedLog(log)}
                  >
                    {new Date(log.createdAt * 1000).toLocaleTimeString()}
                  </button>
                ))}
              </div>
              {selectedLog && (
                <div className="debug-log-detail">
                  <h4>Request</h4>
                  <pre className="debug-json">{formatJson(selectedLog.requestJson)}</pre>
                  <h4>Response</h4>
                  <pre className="debug-json">
                    {selectedLog.responseJson ? formatJson(selectedLog.responseJson) : "No response"}
                  </pre>
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
