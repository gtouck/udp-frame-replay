import { useEffect, useState } from "react";
import { networkInterfaces, type InterfaceInfo } from "../../api";
import { useStore } from "../../store";
import { Check, Field, Hint, NumberField, Segments, TextField } from "./Field";

export default function TargetSection() {
  const target = useStore((s) => s.target);
  const setTarget = useStore((s) => s.setTarget);
  const setTargetMode = useStore((s) => s.setTargetMode);

  const [ifaces, setIfaces] = useState<InterfaceInfo[]>([]);
  useEffect(() => {
    networkInterfaces().then(setIfaces).catch(() => setIfaces([]));
  }, []);

  return (
    <>
      <Segments
        value={target.mode}
        onChange={setTargetMode}
        options={[
          { value: "unicast", label: "单播" },
          { value: "multicast", label: "组播" },
        ]}
      />

      {target.mode === "unicast" ? (
        <TextField
          label="目标 IP"
          value={target.host}
          onChange={(host) => setTarget({ host } as never)}
          placeholder="127.0.0.1"
        />
      ) : (
        <>
          <TextField
            label="组播地址"
            value={target.group}
            onChange={(group) => setTarget({ group } as never)}
            placeholder="239.255.0.1"
          />

          <Field label="出站网卡" htmlFor="cfg-iface">
            <select
              id="cfg-iface"
              className="select"
              value={target.interface ?? ""}
              onChange={(e) =>
                setTarget({ interface: e.target.value || null } as never)
              }
            >
              <option value="">系统默认路由</option>
              {ifaces.map((i) => (
                <option key={`${i.name}-${i.ip}`} value={i.ip}>
                  {i.name} · {i.ip}
                  {i.isLoopback ? "（回环）" : ""}
                </option>
              ))}
            </select>
          </Field>

          <NumberField
            label="TTL"
            value={target.ttl}
            min={1}
            max={255}
            onChange={(ttl) => setTarget({ ttl } as never)}
          />

          <Check
            label="本机也收到自己发的包"
            checked={target.loopback}
            onChange={(loopback) => setTarget({ loopback } as never)}
          />

          <Hint>
            多网卡机器上不指定出站网卡，组播很可能从错误的网卡发出去。
            不确定就挑数据网所在的那张。
          </Hint>
        </>
      )}

      <NumberField
        label="端口"
        value={target.port}
        min={1}
        max={65535}
        onChange={(port) => setTarget({ port } as never)}
      />

      <TextField
        label="本地地址"
        value={target.bindAddr ?? ""}
        onChange={(v) => setTarget({ bindAddr: v || null })}
        placeholder="留空为 0.0.0.0"
      />

      <NumberField
        label="本地端口"
        value={target.bindPort ?? 0}
        min={0}
        max={65535}
        onChange={(v) => setTarget({ bindPort: v || null })}
      />

      <Hint>本地端口填 0 由系统分配。有些接收端会校验源端口。</Hint>
    </>
  );
}
