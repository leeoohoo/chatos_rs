import { Form, Input, InputNumber, Select, Switch } from 'antd';
import type { ConfigDefinition, ConfigValue } from './types';

export function parseJsonConfigInput(text: string):
  { value: ConfigValue; error?: never } | { value?: never; error: string } {
  try {
    return { value: JSON.parse(text) as ConfigValue };
  } catch {
    return { error: '请输入有效的 JSON，修正后才能保存或发布。' };
  }
}

export function ConfigInput({
  definition,
  value,
  onChange,
  jsonDraft,
  onJsonDraftChange,
}: {
  definition: ConfigDefinition;
  value: ConfigValue;
  onChange: (value: ConfigValue) => void;
  jsonDraft?: string;
  onJsonDraftChange: (text: string) => void;
}) {
  if (definition.value_type === 'json') {
    const text = jsonDraft ?? JSON.stringify(value, null, 2);
    const parsed = parseJsonConfigInput(text);
    return (
      <Form.Item validateStatus={parsed.error ? 'error' : undefined} help={parsed.error}>
        <Input.TextArea
          aria-label={definition.display_name}
          aria-invalid={Boolean(parsed.error)}
          value={text}
          autoSize={{ minRows: 8, maxRows: 32 }}
          spellCheck={false}
          style={{ fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace' }}
          onChange={(event) => {
            const next = event.target.value;
            onJsonDraftChange(next);
            const result = parseJsonConfigInput(next);
            if (result.error === undefined) onChange(result.value);
          }}
        />
      </Form.Item>
    );
  }
  if (definition.value_type === 'boolean') {
    return <Switch checked={value === true} onChange={onChange} />;
  }
  if (definition.value_type === 'enum') {
    return (
      <Select
        value={typeof value === 'string' ? value : undefined}
        onChange={onChange}
        style={{ width: '100%' }}
        options={definition.enum_options.map((item) => ({ value: item, label: item }))}
      />
    );
  }
  if (['integer', 'duration_ms', 'bytes'].includes(definition.value_type)) {
    return (
      <InputNumber
        value={typeof value === 'number' ? value : null}
        min={definition.min ?? undefined}
        max={definition.max ?? undefined}
        onChange={(next) => onChange(next)}
        style={{ width: '100%' }}
        addonAfter={definition.value_type === 'duration_ms' ? 'ms' : definition.value_type === 'bytes' ? 'bytes' : undefined}
      />
    );
  }
  return (
    <Input
      value={typeof value === 'string' ? value : ''}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}
