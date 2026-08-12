import type { Delimiter, TextEncoding } from "../api";
import { useStore } from "../store";
import Group from "./Group";

export default function ConfigPanel() {
  const parse = useStore((s) => s.parse);
  const setParse = useStore((s) => s.setParse);
  const setPrefix = useStore((s) => s.setPrefix);
  const setPrefixMode = useStore((s) => s.setPrefixMode);
  const setSkipChars = useStore((s) => s.setSkipChars);

  const prefix = parse.prefix;

  return (
    <aside className="config">
      <Group name="解析规则">
        <div className="field">
          <label className="field-label" htmlFor="cfg-enc">
            编码
          </label>
          <select
            id="cfg-enc"
            className="select"
            value={parse.encoding}
            onChange={(e) =>
              setParse({ encoding: e.target.value as TextEncoding })
            }
          >
            <option value="utf8">UTF-8</option>
            <option value="gbk">GBK（简体中文）</option>
            <option value="latin1">Latin-1</option>
          </select>
        </div>

        <div className="segments">
          <button
            className="segment"
            aria-pressed={prefix.mode === "fields"}
            onClick={() => setPrefixMode("fields")}
          >
            按字段丢弃
          </button>
          <button
            className="segment"
            aria-pressed={prefix.mode === "chars"}
            onClick={() => setPrefixMode("chars")}
          >
            按字符跳过
          </button>
        </div>

        {prefix.mode === "fields" ? (
          <>
            <div className="field">
              <label className="field-label" htmlFor="cfg-delim">
                分隔符
              </label>
              <select
                id="cfg-delim"
                className="select"
                value={prefix.delimiter.kind}
                onChange={(e) => {
                  const kind = e.target.value as Delimiter["kind"];
                  setPrefix({
                    delimiter:
                      kind === "custom"
                        ? { kind: "custom", value: "|" }
                        : ({ kind } as Delimiter),
                  });
                }}
              >
                <option value="whitespace">空白（空格 / Tab）</option>
                <option value="comma">逗号</option>
                <option value="tab">Tab</option>
                <option value="custom">自定义</option>
              </select>
            </div>

            {prefix.delimiter.kind === "custom" && (
              <div className="field">
                <label className="field-label" htmlFor="cfg-delim-chars">
                  分隔字符
                </label>
                <input
                  id="cfg-delim-chars"
                  className="input"
                  value={prefix.delimiter.value}
                  onChange={(e) =>
                    setPrefix({
                      delimiter: { kind: "custom", value: e.target.value },
                    })
                  }
                />
              </div>
            )}

            <div className="field">
              <label className="field-label" htmlFor="cfg-skip-fields">
                丢弃
              </label>
              <input
                id="cfg-skip-fields"
                className="input"
                type="number"
                min={0}
                value={prefix.skipFields}
                onChange={(e) =>
                  setPrefix({ skipFields: Math.max(0, +e.target.value || 0) })
                }
              />
            </div>

            <label className="check">
              <input
                type="checkbox"
                checked={prefix.collapse}
                onChange={(e) => setPrefix({ collapse: e.target.checked })}
              />
              连续分隔符算一个
            </label>

            <p className="hint">
              丢弃行首的 {prefix.skipFields} 个字段，其余部分作为数据。
            </p>
          </>
        ) : (
          <>
            <div className="field">
              <label className="field-label" htmlFor="cfg-skip-chars">
                跳过
              </label>
              <input
                id="cfg-skip-chars"
                className="input"
                type="number"
                min={0}
                value={prefix.skipChars}
                onChange={(e) => setSkipChars(Math.max(0, +e.target.value || 0))}
              />
            </div>
            <p className="hint">
              跳过行首 {prefix.skipChars} 个字符。按字符计，一个汉字算一个。
            </p>
          </>
        )}

        <div className="field">
          <label className="field-label" htmlFor="cfg-ignore">
            忽略字符
          </label>
          <input
            id="cfg-ignore"
            className="input"
            value={parse.hex.ignoreChars}
            onChange={(e) => setParse({ hex: { ignoreChars: e.target.value } })}
          />
        </div>

        <p className="hint">
          数据中的空白和这些字符会被跳过。遇到其他非十六进制字符时数据到此为止，
          其后按尾注处理。
        </p>
      </Group>
    </aside>
  );
}
