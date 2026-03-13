import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvents } from "../hooks/useTauriEvents";

interface ModelInfo {
  name: string;
  disk_size_mb: number;
  ram_mb: number;
  is_downloaded: boolean;
}

export function ModelManager({ activeModel }: { activeModel: string }) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [downloading, setDownloading] = useState<Record<string, number>>({});
  const [active, setActive] = useState(activeModel);

  const refresh = () =>
    invoke<ModelInfo[]>("list_models").then(setModels);

  useEffect(() => {
    refresh();
  }, []);

  useTauriEvents((event) => {
    if (event.type === "download-progress") {
      setDownloading((prev) => ({
        ...prev,
        [event.model]: event.percent,
      }));
      if (event.percent >= 100) {
        setTimeout(() => {
          setDownloading((prev) => {
            const next = { ...prev };
            delete next[event.model];
            return next;
          });
          refresh();
        }, 500);
      }
    }
  });

  const download = async (name: string) => {
    setDownloading((prev) => ({ ...prev, [name]: 0 }));
    try {
      await invoke("download_model", { model: name });
    } catch (e) {
      setDownloading((prev) => {
        const next = { ...prev };
        delete next[name];
        return next;
      });
      alert(`Download failed: ${e}`);
    }
    refresh();
  };

  const remove = async (name: string) => {
    if (!confirm(`Delete model "${name}"?`)) return;
    await invoke("delete_model", { model: name });
    refresh();
  };

  const activate = async (name: string) => {
    await invoke("set_active_model", { model: name });
    setActive(name);
  };

  return (
    <div style={{ marginTop: 32 }}>
      <h3
        style={{
          fontSize: 13,
          fontWeight: 600,
          color: "#8e8e93",
          textTransform: "uppercase",
          letterSpacing: "0.06em",
          margin: "0 0 12px",
        }}
      >
        Whisper Models
      </h3>

      {models.map((m) => {
        const inProgress = downloading[m.name] !== undefined;
        const pct = downloading[m.name] ?? 0;

        return (
          <div key={m.name} className="model-row">
            <div>
              <span style={{ fontWeight: 600 }}>{m.name}</span>
              {active === m.name && (
                <span
                  style={{
                    marginLeft: 8,
                    fontSize: 11,
                    color: "#30d158",
                    fontWeight: 600,
                  }}
                >
                  ✓ Active
                </span>
              )}
              <div style={{ fontSize: 12, color: "#8e8e93", marginTop: 2 }}>
                {m.disk_size_mb >= 1000
                  ? `${(m.disk_size_mb / 1024).toFixed(1)} GB`
                  : `${m.disk_size_mb} MB`}{" "}
                · ~{m.ram_mb >= 1024 ? `${(m.ram_mb / 1024).toFixed(1)} GB` : `${m.ram_mb} MB`} RAM
              </div>
              {inProgress && (
                <div className="progress-bar-bg">
                  <div
                    className="progress-bar-fill"
                    style={{ width: `${pct}%` }}
                  />
                </div>
              )}
            </div>

            <div style={{ display: "flex", gap: 8 }}>
              {!m.is_downloaded && !inProgress && (
                <button className="btn-secondary" onClick={() => download(m.name)}>
                  Download
                </button>
              )}
              {inProgress && (
                <span style={{ fontSize: 13, color: "#8e8e93" }}>{pct}%</span>
              )}
              {m.is_downloaded && active !== m.name && (
                <button className="btn-secondary" onClick={() => activate(m.name)}>
                  Use
                </button>
              )}
              {m.is_downloaded && (
                <button className="btn-danger" onClick={() => remove(m.name)}>
                  Delete
                </button>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
