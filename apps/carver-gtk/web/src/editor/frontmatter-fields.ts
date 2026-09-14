function jsonObject(source: string): Record<string, unknown> | null {
  try {
    const value: unknown = JSON.parse(source);
    return value !== null && !Array.isArray(value) && typeof value === 'object'
      ? (value as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

/** Returns a supported text field without interpreting JSON as YAML. */
export function frontmatterFieldValue(
  source: string,
  key: string,
  format: string,
): string {
  if (format === 'json') {
    const value = jsonObject(source)?.[key];
    return typeof value === 'string' ? value : '';
  }

  const separator = format === 'toml' ? '=' : ':';
  const match = source.match(
    new RegExp(`^${key}\\s*${separator}\\s*(.*?)\\s*$`, 'm'),
  );
  if (!match) return '';
  const value = match[1];
  if (value.startsWith('"') && value.endsWith('"')) {
    try {
      return JSON.parse(value) as string;
    } catch {
      return value.slice(1, -1);
    }
  }
  if (value.startsWith("'") && value.endsWith("'")) {
    return value.slice(1, -1).replace(/''/g, "'");
  }
  return value;
}

/** Updates a supported text field while preserving valid JSON syntax. */
export function setFrontmatterField(
  source: string,
  key: string,
  value: string,
  format: string,
): string {
  if (format === 'json') {
    const fields = jsonObject(source);
    if (!fields) return source;
    if (value) fields[key] = value;
    else delete fields[key];
    return JSON.stringify(fields, null, 2);
  }

  const separator = format === 'toml' ? '=' : ':';
  const line =
    format === 'toml'
      ? `${key} = ${JSON.stringify(value)}`
      : `${key}: ${JSON.stringify(value)}`;
  const pattern = new RegExp(`^${key}\\s*${separator}.*(?:\\n|$)`, 'm');
  if (pattern.test(source))
    return source.replace(pattern, value ? `${line}\n` : '');
  if (!value) return source;
  if (format === 'toml') {
    const table = source.search(/^\s*\[/m);
    if (table >= 0) {
      const prefix = source.slice(0, table).replace(/\n*$/, '');
      return `${prefix}${prefix ? '\n' : ''}${line}\n${source.slice(table)}`;
    }
  }
  return source ? `${source.replace(/\n+$/, '')}\n${line}` : line;
}

/** Reports whether the common metadata controls can safely update the source. */
export function frontmatterFieldsAreEditable(
  source: string,
  format: string,
): boolean {
  return format !== 'json' || jsonObject(source) !== null;
}
