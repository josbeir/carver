import { execFileSync } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { compile } from 'json-schema-to-typescript';

const webRoot = new URL('../', import.meta.url);
const output = new URL('src/editor/protocol.generated.ts', webRoot);
const schema = JSON.parse(
  execFileSync(
    'cargo',
    [
      'run',
      '--quiet',
      '--locked',
      '-p',
      'carver-editor-protocol',
      '--features',
      'json-schema',
      '--example',
      'export-editor-schema',
    ],
    { cwd: fileURLToPath(webRoot), encoding: 'utf8' },
  ),
);
const types = await compile(schema, 'EditorEvent', {
  additionalProperties: false,
  bannerComment:
    '// Generated from carver-editor-protocol. Run npm run protocol:generate; do not edit.',
});
const formatted = execFileSync(
  fileURLToPath(new URL('node_modules/.bin/biome', webRoot)),
  ['format', '--stdin-file-path=src/editor/protocol.generated.ts'],
  { cwd: fileURLToPath(webRoot), input: types, encoding: 'utf8' },
);
if (process.argv.includes('--check')) {
  if ((await readFile(output, 'utf8')) !== formatted) {
    throw new Error(
      'Editor protocol types are stale. Run npm run protocol:generate.',
    );
  }
} else {
  await writeFile(output, formatted);
}
