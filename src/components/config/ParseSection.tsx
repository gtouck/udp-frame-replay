import type { Delimiter, TextEncoding } from "../../api";
import { useStore } from "../../store";
import { Check, Field, Hint, NumberField, Segments, TextField } from "./Field";

export default function ParseSection() {
  const parse = useStore((s) => s.parse);
  const setParse = useStore((s) => s.setParse);
  const setPrefix = useStore((s) => s.setPrefix);
  const setPrefixMode = useStore((s) => s.setPrefixMode);
  const setSkipChars = useStore((s) => s.setSkipChars);

  const prefix = parse.prefix;

  return (
    <>
      <Field label="编码" htmlFor="cfg-enc">
        <select
          id="cfg-enc"
          className="select"
          value={parse.encoding}
          onChange={(e) => setParse({ encoding: e.target.value as TextEncoding })}
        >
          <option value="utf8">UTF-8</option>
          <option value="gbk">GBK（简体中文）</option>
          <option value="latin1">Latin-1</option>
        </select>
      </Field>

      <Segments
        value={prefix.mode}
        onChange={setPrefixMode}
        options={[
          { value: "fields", label: "按字段丢弃" },
          { value: "chars", label: "按字符跳过" },
        ]}
      />

      {prefix.mode === "fields" ? (
        <>
          <Field label="分隔符" htmlFor="cfg-delim">
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
          </Field>

          {prefix.delimiter.kind === "custom" && (
            <TextField
              label="分隔字符"
              value={prefix.delimiter.value}
              onChange={(v) =>
                setPrefix({ delimiter: { kind: "custom", value: v } })
              }
            />
          )}

          <NumberField
            label="丢弃"
            value={prefix.skipFields}
            onChange={(v) => setPrefix({ skipFields: v })}
            suffix="个字段"
          />

          <Check
            label="连续分隔符算一个"
            checked={prefix.collapse}
            onChange={(v) => setPrefix({ collapse: v })}
          />

          <Hint>
            {prefix.skipFields === 0
              ? "整行都作为数据。"
              : `丢弃行首的 ${prefix.skipFields} 个字段，其余部分作为数据。`}
          </Hint>
        </>
      ) : (
        <>
          <NumberField
            label="跳过"
            value={prefix.skipChars}
            onChange={setSkipChars}
            suffix="个字符"
          />
          <Hint>
            跳过行首 {prefix.skipChars} 个字符。按字符计，一个汉字算一个。
          </Hint>
        </>
      )}

      <TextField
        label="忽略字符"
        value={parse.hex.ignoreChars}
        onChange={(v) => setParse({ hex: { ignoreChars: v } })}
      />

      <Hint>
        数据中的空白和这些字符会被跳过。遇到其他非十六进制字符时数据到此为止，
        其后按尾注处理。
      </Hint>
    </>
  );
}
