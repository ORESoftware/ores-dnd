// One command for the whole cross-runtime gate:
//   node scripts/conformance/run.mjs [--skip dart] [--skip rust] [--only typescript]
// 1. write the trusted case list; 2. run every available runtime adapter;
// 3. verify the evidence against the current authorities (verify.mjs).
import { spawn } from "node:child_process";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { OUT_DIR, ROOT, writeCases } from "./corpus.mjs";
import { ADAPTERS, verify } from "./verify.mjs";

function run(cmd, args, cwd) {
  return new Promise((resolvePromise) => {
    const child = spawn(cmd, args, { cwd, stdio: ["ignore", "inherit", "inherit"], env: process.env });
    child.on("error", (error) => resolvePromise({ ok: false, error: error.message }));
    child.on("exit", (code) => resolvePromise({ ok: code === 0, code }));
  });
}

const argv = process.argv.slice(2);
const skip = new Set(argv.flatMap((a, i) => (a === "--skip" ? [argv[i + 1]] : a.startsWith("--skip=") ? [a.slice(7)] : [])));
const only = argv.flatMap((a, i) => (a === "--only" ? [argv[i + 1]] : a.startsWith("--only=") ? [a.slice(7)] : []));
const wanted = (only.length ? only : Object.keys(ADAPTERS)).filter((name) => !skip.has(name));

await mkdir(OUT_DIR, { recursive: true });
const { cases, path: casesPath } = await writeCases();
console.log(`conformance: ${cases.length} cases -> ${casesPath}`);

const commands = {
  typescript: ["node", ["conformance-adapter.mjs", casesPath, join(OUT_DIR, "evidence-typescript.json")], join(ROOT, "src", "ts")],
  rust: ["cargo", ["run", "-q", "-p", "ores-dnd-core", "--example", "conformance", "--", casesPath, join(OUT_DIR, "evidence-rust.json")], ROOT],
  dart: ["dart", ["run", "bin/conformance_adapter.dart", casesPath, join(OUT_DIR, "evidence-dart.json")], join(ROOT, "src", "dart")],
};

const ran = [];
for (const name of wanted) {
  const [cmd, args, cwd] = commands[name];
  console.log(`conformance: running ${name} adapter (${cmd} ${args.join(" ")})`);
  const result = await run(cmd, args, cwd);
  if (!result.ok) {
    console.log(`conformance: ${name} adapter FAILED (${result.error ?? `exit ${result.code}`})`);
    process.exitCode = 2;
  } else {
    ran.push(name);
  }
}
if (ran.length !== wanted.length) {
  console.log("conformance: not every required adapter produced evidence; verification not attempted");
  process.exit(2);
}
const result = await verify({ require: wanted });
if (result.status === "passed") {
  console.log(`runtime conformance: PASSED — ${result.cases} cases × ${result.adapters.length} adapters (${result.adapters.join(", ")})`);
} else {
  console.log(`runtime conformance: ${result.status.toUpperCase()} at ${result.stage}`);
  for (const finding of result.findings ?? []) console.log(`- [${finding.ruleId}] ${finding.message}${finding.pointer ? ` at ${finding.pointer}` : ""}`);
  process.exitCode = 2;
}
