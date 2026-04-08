import type { DynamicConfigRecord, DynamicConfigValue } from './types';

interface DynamicConfigFieldsProps {
  config: DynamicConfigRecord;
  onChange: (key: string, value: DynamicConfigValue) => void;
}

const DynamicConfigFields = ({ config, onChange }: DynamicConfigFieldsProps) => {
  const entries = Object.entries(config);

  if (entries.length === 0) {
    return null;
  }

  return (
    <div className="grid grid-cols-1 gap-3">
      {entries.map(([key, value]) => {
        const valueType = typeof value;
        const isArray = Array.isArray(value);

        return (
          <div key={key}>
            <label className="block text-xs text-muted-foreground mb-1">{key}</label>
            {valueType === 'boolean' ? (
              <div className="flex items-center">
                <input
                  type="checkbox"
                  checked={Boolean(value)}
                  onChange={(event) => onChange(key, event.target.checked)}
                  className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
                />
                <span className="ml-2 text-xs">{String(value)}</span>
              </div>
            ) : isArray ? (
              <input
                type="text"
                value={value.join(', ')}
                onChange={(event) =>
                  onChange(
                    key,
                    event.target.value
                      .split(',')
                      .map((item) => item.trim())
                      .filter(Boolean),
                  )
                }
                className="w-full px-2 py-1 border border-input bg-background text-foreground rounded-md"
              />
            ) : (
              <input
                type={valueType === 'number' ? 'number' : 'text'}
                value={typeof value === 'string' || typeof value === 'number' ? value : ''}
                onChange={(event) =>
                  onChange(
                    key,
                    valueType === 'number' ? Number(event.target.value) : event.target.value,
                  )
                }
                className="w-full px-2 py-1 border border-input bg-background text-foreground rounded-md"
              />
            )}
          </div>
        );
      })}
    </div>
  );
};

export default DynamicConfigFields;
