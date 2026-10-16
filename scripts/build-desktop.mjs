import { spawnSync } from "node:child_process";

// Apple Silicon executables need a signature. Sign the whole local bundle so
// its resource envelope is consistent; a supplied Developer ID takes priority.
const buildEnvironment = { ...process.env };
if (process.platform === "darwin" && !buildEnvironment.APPLE_SIGNING_IDENTITY) {
  buildEnvironment.APPLE_SIGNING_IDENTITY = "-";
}
const result = spawnSync(
  "pnpm",
  ["exec", "tauri", "build", ...process.argv.slice(2)],
  {
    env: buildEnvironment,
    stdio: "inherit",
  },
);
if (result.error) throw result.error;
process.exit(result.status ?? 1);
