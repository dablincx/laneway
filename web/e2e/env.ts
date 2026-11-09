import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";

// The one-time setup token comes from the server log. Where to read it is configurable:
//   LANEWAY_E2E_SETUP_TOKEN   the token itself
//   LANEWAY_E2E_LOG_FILE      a file with the server's log output
//   LANEWAY_E2E_LOG_COMMAND   a shell command that prints the log, e.g. `docker logs <container>`
const pattern = /setup token: ([A-Za-z0-9_-]+)/g;

function serverLog(): string {
  const file = process.env.LANEWAY_E2E_LOG_FILE;
  if (file) return readFileSync(file, "utf8");
  const command = process.env.LANEWAY_E2E_LOG_COMMAND;
  // docker logs writes the container's stderr to stderr, so read both.
  if (command) return execSync(`${command} 2>&1`, { encoding: "utf8" });
  throw new Error("set LANEWAY_E2E_SETUP_TOKEN, LANEWAY_E2E_LOG_FILE or LANEWAY_E2E_LOG_COMMAND");
}

/** The latest setup token the server printed. */
export function setupToken(): string {
  const direct = process.env.LANEWAY_E2E_SETUP_TOKEN;
  if (direct) return direct;
  // Strip ANSI colors so the token isn't glued to an escape code.
  // biome-ignore lint/suspicious/noControlCharactersInRegex: matches ANSI escapes on purpose
  const log = serverLog().replace(/\x1b\[[0-9;]*m/g, "");
  const token = [...log.matchAll(pattern)].map((m) => m[1]).at(-1);
  if (!token) throw new Error("no setup token found in the server log");
  return token;
}
