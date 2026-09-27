import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

/**
 * Carver's version, read from the workspace `Cargo.toml` at build time so the
 * install instructions and download links never drift from the release.
 *
 * The lookup walks up from the working directory (the `docs/` project root)
 * rather than using `import.meta.url`, because this module is bundled into
 * Astro's prerender output where a module-relative path would no longer point
 * at the repository.
 */
function readWorkspaceVersion(): string {
  let directory = process.cwd();

  for (let depth = 0; depth < 5; depth += 1) {
    try {
      const cargoToml = readFileSync(resolve(directory, 'Cargo.toml'), 'utf8');
      const match = cargoToml.match(/\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/);
      if (match) {
        return match[1];
      }
    } catch {
      // No Cargo.toml here; keep walking toward the repository root.
    }

    const parent = dirname(directory);
    if (parent === directory) {
      break;
    }
    directory = parent;
  }

  throw new Error('Could not read the workspace version from Cargo.toml');
}

export const version = readWorkspaceVersion();
