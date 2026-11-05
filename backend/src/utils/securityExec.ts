import { execFile } from "child_process";

type ExecFileSafeOptions = {
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  timeout?: number;
};

const EXTRA_POSIX_BIN_DIRS = ["/opt/homebrew/bin", "/usr/local/bin"];

function buildAugmentedEnv(customEnv?: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const baseEnv = { ...process.env, ...(customEnv || {}) };
  if (process.platform !== "win32") {
    const currentPath = baseEnv.PATH || "/usr/bin:/bin:/usr/sbin:/sbin";
    const segments = currentPath.split(":").filter(Boolean);
    for (const dir of EXTRA_POSIX_BIN_DIRS) {
      if (!segments.includes(dir)) {
        segments.unshift(dir);
      }
    }
    baseEnv.PATH = segments.join(":");
  }
  return baseEnv;
}

/**
 * Safely execute a command with arguments
 * Prevents command injection by using execFile instead of exec
 */
export function execFileSafe(
  command: string,
  args: string[],
  options?: ExecFileSafeOptions
): Promise<{ stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    const effectiveOptions: ExecFileSafeOptions = {
      ...options,
      env: buildAugmentedEnv(options?.env),
    };
    execFile(command, args, effectiveOptions, (error, stdout, stderr) => {
      if (error) {
        reject(error);
      } else {
        resolve({ stdout: stdout.toString(), stderr: stderr.toString() });
      }
    });
  });
}
