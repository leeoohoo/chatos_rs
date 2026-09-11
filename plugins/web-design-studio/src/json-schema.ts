export function stringLiteralSchema<const Value extends string>(value: Value) {
  return { type: 'string', enum: [value] } as const;
}

export const jsonScalarValueSchema = {
  anyOf: [
    { type: 'string' },
    { type: 'number' },
    { type: 'boolean' },
    { type: 'null' }
  ]
} as const;

export const jsonEncodedValueSchema = {
  type: 'string',
  minLength: 1,
  maxLength: 100_000,
  description: 'A JSON-encoded object or array value. Use the scalar value field for strings, numbers, booleans, or null.'
} as const;
