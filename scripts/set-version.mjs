// Stamps a release version (and the update feed of the repository being built) into the app
// config. CI runs this before building; local builds keep the version in the files.
//
//   node scripts/set-version.mjs 0.1.42
//
// GITHUB_REPOSITORY (owner/name, set by GitHub Actions) replaces the OWNER/gitwise placeholder in
// the updater endpoint, so a fork or a renamed repository updates from its own releases.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error(`usage: set-version.mjs <major.minor.patch>, got "${version}"`);
  process.exit(1);
}

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const confPath = join(root, "apps/desktop/src-tauri/tauri.conf.json");
const pkgPath = join(root, "apps/desktop/package.json");

const conf = JSON.parse(readFileSync(confPath, "utf8"));
conf.version = version;
const repo = process.env.GITHUB_REPOSITORY;
if (repo) {
  conf.plugins.updater.endpoints = conf.plugins.updater.endpoints.map((u) => u.replace(/github\.com\/[^/]+\/[^/]+\/releases/, `github.com/${repo}/releases`));
}
writeFileSync(confPath, JSON.stringify(conf, null, 2) + "\n");

const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
pkg.version = version;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

console.log(`version ${version}${repo ? `, updates from ${repo}` : ""}`);
