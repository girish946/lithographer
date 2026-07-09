import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');
const script = process.argv[2];
const extraArgs = process.argv.slice(3);
const isWin = process.platform === 'win32';

const scripts = {
  'prepare-sidecar': {
    win: path.join(root, 'src-tauri/scripts/prepare-litho-sidecar.ps1'),
    unix: path.join(root, 'src-tauri/scripts/prepare-litho-sidecar.sh'),
  },
  'vendor-assets': {
    win: path.join(root, 'scripts/vendor-frontend-assets.ps1'),
    unix: path.join(root, 'scripts/vendor-frontend-assets.sh'),
  },
  'tauri-build': {
    win: path.join(root, 'scripts/tauri-build.ps1'),
    unix: path.join(root, 'scripts/tauri-build.sh'),
  },
  'generate-icons': {
    win: path.join(root, 'scripts/generate-app-icons.ps1'),
    unix: path.join(root, 'scripts/generate-app-icons.sh'),
  },
};

const entry = scripts[script];
if (!entry) {
  console.error(`Unknown script: ${script ?? '(none)'}`);
  process.exit(1);
}

const result = isWin
  ? spawnSync(
      'powershell',
      ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', entry.win, ...extraArgs],
      { stdio: 'inherit', cwd: root, shell: false },
    )
  : spawnSync('bash', [entry.unix, ...extraArgs], { stdio: 'inherit', cwd: root, shell: false });

process.exit(result.status ?? 1);