import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

if (process.platform !== 'linux') {
  process.exit(0);
}

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');
const patchScript = path.join(root, 'src-tauri/scripts/patch-appimage-wayland.sh');

const result = spawnSync('bash', [patchScript], { stdio: 'inherit', cwd: root, shell: false });
process.exit(result.status ?? 1);