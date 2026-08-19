import { join } from "@tauri-apps/api/path";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { appDir, loadProfile, saveProfile } from "../../api";
import { configOf, useStore } from "../../store";
import { Hint, TextField } from "./Field";

/**
 * 存取配置档时的起始位置：程序所在目录。
 *
 * 便携版是「一个文件夹装下全部」的用法，配置档理应躺在程序旁边；
 * 系统默认的「文档」目录反而要人多翻两层。取不到就返回 undefined，
 * 让对话框自己决定 —— 这只是个默认值，不值得为它中断保存。
 */
async function defaultPath(fileName?: string): Promise<string | undefined> {
  try {
    const dir = await appDir();
    return fileName ? await join(dir, fileName) : dir;
  } catch {
    return undefined;
  }
}

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
      defaultPath: await defaultPath(`${name || "配置"}.json`),
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
      defaultPath: await defaultPath(),
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
        解析、筛选、修改、目标、节奏五部分一起存成一个 JSON 文件，默认存到程序所在目录，
        换台机器也能直接用。日常不用特意保存 —— 配置本来就会自动记住，下次打开还在。
      </Hint>
    </>
  );
}
