import { readFile, readdir, writeFile, mkdir } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import path from "node:path";
const output = [
  "THIRD-PARTY LICENSE TEXTS\nGenerated from locked dependency packages; raw notices follow.\n",
];
const lock = JSON.parse(await readFile("package-lock.json", "utf8"));
for (const [location, info] of Object.entries(lock.packages)) {
  if (!location) continue;
  try {
    const names = await readdir(location);
    const licenses = names.filter((n) =>
      /^(licen[cs]e|copying|notice)(\.|$)/i.test(n),
    );
    for (const name of licenses) {
      const text = await readFile(path.join(location, name), "utf8");
      output.push(
        `\n====== ${location} ${info.version} (${info.license || "see text"}) / ${name} ======\n${text}\n`,
      );
    }
  } catch {}
}
const cargo = JSON.parse(
  execFileSync(
    "cargo",
    [
      "metadata",
      "--locked",
      "--format-version",
      "1",
      "--filter-platform",
      "x86_64-pc-windows-msvc",
      "--manifest-path",
      "src-tauri/Cargo.toml",
    ],
    { encoding: "utf8", maxBuffer: 20 * 1024 * 1024 },
  ),
);
for (const pkg of cargo.packages) {
  const dir = path.dirname(pkg.manifest_path);
  const names = await readdir(dir);
  const licenses = names.filter((n) =>
    /^(licen[cs]e|copying|notice)(\.|$)/i.test(n),
  );
  output.push(
    `\n====== Rust ${pkg.name} ${pkg.version}; license: ${pkg.license || "see source"}; ${pkg.repository || ""} ======\n`,
  );
  for (const name of licenses) {
    try {
      output.push(`${name}\n${await readFile(path.join(dir, name), "utf8")}\n`);
    } catch {}
  }
}
await mkdir("docs/licenses", { recursive: true });
await writeFile("docs/licenses/DEPENDENCY-LICENSES.txt", output.join(""));
console.log("Collected npm and Cargo license texts.");
