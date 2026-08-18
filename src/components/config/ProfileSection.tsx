import { open, save } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { loadProfile, saveProfile } from "../../api";
import { configOf, useStore } from "../../store";
import { Hint, TextField } from "./Field";

/**
 * 配置档存取。
 *
 * 既然走的是手动规则路线，配置复用就是刚需 —— 换一种数据格式不该从头再配一遍。
 *
 * 与自动记忆（`useSessionPersist`）分工不同：那边负责「关掉再打开还在」，
 * 这边负责「命名存档、带到别的机器上」。
 */
export default function ProfileSection() {
  const applyConfig = useStore((s) => s.applyConfig);
  const [name, setName] = useState("");
  const [note, setNote] = useState<string | null>(null);

  async function doSave() {
    const path = await save({
      defaultPath: `${name || "配置"}.json`,
      filters: [{ name: "配置档", extensions: ["json"] }],
    });
    if (!path) return;

    try {
      await saveProfile(path, name || "未命名", configOf(useStore.getState()));
      setNote(`已保存到 ${path}`);
    } catch (e) {
      setNote(String(e));
    }
  }

  async function doLoad() {
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "配置档", extensions: ["json"] }],
    });
    if (typeof picked !== "string") return;

    try {
      const p = await loadProfile(picked);
      applyConfig(p.config);
      setName(p.name);
      setNote(`已载入「${p.name}」`);
    } catch (e) {
      setNote(String(e));
    }
  }

  return (
    <>
      <TextField label="名称" value={name} onChange={setName} placeholder="未命名" />

      <div className="rule-add">
        <button className="btn btn-slim" onClick={doSave}>
          保存到文件
        </button>
        <button className="btn btn-slim" onClick={doLoad}>
          从文件载入
        </button>
      </div>

      {note && <Hint>{note}</Hint>}

      <Hint>
        解析、筛选、修改、目标、节奏五部分一起存成一个 JSON 文件，换台机器也能直接用。
        日常不用特意保存 —— 配置本来就会自动记住，下次打开还在。
      </Hint>
    </>
  );
}
