#!/usr/bin/env node
// Thin launcher: the real binary ships in a per-platform optionalDependency
// (biome/esbuild model). This shim resolves it and execs through.
const { spawnSync } = require("node:child_process")

// npm's spam filter rejects "win32" in package names, so Windows ships as
// effect-doctor-windows-x64.
const platformName = process.platform === "win32" ? "windows" : process.platform
const platformPackage = `effect-doctor-${platformName}-${process.arch}`
const binaryName = process.platform === "win32" ? "effect-doctor.exe" : "effect-doctor"

let binaryPath
try {
  binaryPath = require.resolve(`${platformPackage}/bin/${binaryName}`)
} catch {
  console.error(`effect-doctor: no prebuilt binary for ${process.platform}-${process.arch}.`)
  console.error("Build from source instead:")
  console.error("  cargo install --git https://github.com/JGalbss/effect-doctor effect-doctor")
  process.exit(1)
}

const result = spawnSync(binaryPath, process.argv.slice(2), { stdio: "inherit" })
if (result.error) {
  console.error(`effect-doctor: failed to launch binary: ${result.error.message}`)
  process.exit(1)
}
process.exit(result.status ?? 1)
