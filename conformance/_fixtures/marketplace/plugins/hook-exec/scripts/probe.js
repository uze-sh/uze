// probe.js <label> <answer> [words...]: the exec-form twin of hook-effects'
// scripts/probe. It records every HOOK_* value it was handed, and the words
// after <answer> as JSON (LAB_WORDS), which reach it only if nothing on the
// way read them as shell. `deny-marked` denies a command or path carrying
// `lab-deny`.
const fs = require("fs");

const [label, answer, ...words] = process.argv.slice(2);
const dir = "/work/hook-markers";
fs.mkdirSync(dir, { recursive: true });
const lines = Object.keys(process.env)
  .filter((key) => key.startsWith("HOOK_"))
  .sort()
  .map((key) => `${key}=${process.env[key]}`);
lines.push(`LAB_WORDS=${JSON.stringify(words)}`);
fs.writeFileSync(
  `${dir}/${label}.${process.env.HOOK_EVENT || "unknown"}.${process.pid}`,
  `${lines.join("\n")}\n`,
);
const subject = `${process.env.HOOK_COMMAND || ""}${process.env.HOOK_PATH || ""}`;
if (answer === "deny-marked" && subject.includes("lab-deny")) {
  process.stderr.write(`lab-hook-denied:${label} tool=${process.env.HOOK_TOOL}`);
  process.exit(3);
}
