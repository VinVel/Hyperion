/**
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */
import semver from "semver";

type PackageJson = {
  workspaces?: {
    catalog?: Record<string, string>;
  };
};

type RegistryMetadata = {
  versions?: Record<string, unknown>;
};

type UpdateCandidate = {
  packageName: string;
  currentSpec: string;
  nextSpec: string;
};

const packageJsonPath = "package.json";
const lockfilePath = "bun.lock";
const npmRegistryUrl = "https://registry.npmjs.org";

// Catalog updates are applied one package at a time so a failed install can be
// rolled back without losing the packages that already resolved cleanly.
const installCommand = ["bun", "install"];

const options = {
  dryRun: Bun.argv.includes("--dry-run"),
  includePinned: Bun.argv.includes("--include-pinned"),
  latest: Bun.argv.includes("--latest"),
  skipInstall: Bun.argv.includes("--skip-install"),
};

const requestedPackageNames = new Set(
  Bun.argv.slice(2).filter((argument) => !argument.startsWith("--")),
);

function readPackageJson(): Promise<PackageJson> {
  return Bun.file(packageJsonPath).json() as Promise<PackageJson>;
}

async function readTextIfPresent(path: string): Promise<string | null> {
  const file = Bun.file(path);

  if (!(await file.exists())) {
    return null;
  }

  return file.text();
}

async function writePackageJson(packageJson: PackageJson): Promise<void> {
  await Bun.write(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`);
}

function getCatalog(packageJson: PackageJson): Record<string, string> {
  const catalog = packageJson.workspaces?.catalog;

  if (catalog === undefined) {
    throw new Error("package.json does not define workspaces.catalog.");
  }

  return catalog;
}

function packageRegistryUrl(packageName: string): string {
  return `${npmRegistryUrl}/${encodeURIComponent(packageName)}`;
}

async function fetchPackageVersions(packageName: string): Promise<string[]> {
  const response = await fetch(packageRegistryUrl(packageName));

  if (!response.ok) {
    throw new Error(
      `Could not fetch ${packageName}: ${response.status} ${response.statusText}`,
    );
  }

  const metadata = (await response.json()) as RegistryMetadata;
  const versionMap = metadata.versions ?? {};

  return Object.keys(versionMap)
    .filter((version) => semver.valid(version) !== null)
    .sort(semver.rcompare);
}

function compatibleRangeFor(currentSpec: string): string | null {
  const currentVersion = semver.minVersion(currentSpec);

  if (currentVersion === null) {
    return null;
  }

  if (currentVersion.major === 0) {
    return `>=${currentVersion.version} <0.${currentVersion.minor + 1}.0`;
  }

  return `>=${currentVersion.version} <${currentVersion.major + 1}.0.0`;
}

function resolveNextVersion(
  currentSpec: string,
  versions: string[],
): string | null {
  if (options.latest) {
    return versions[0] ?? null;
  }

  const compatibleRange = compatibleRangeFor(currentSpec);

  if (compatibleRange === null) {
    return null;
  }

  return semver.maxSatisfying(versions, compatibleRange);
}

function preserveRangePrefix(currentSpec: string, nextVersion: string): string {
  if (currentSpec.startsWith("^")) {
    return `^${nextVersion}`;
  }

  if (currentSpec.startsWith("~")) {
    return `~${nextVersion}`;
  }

  return nextVersion;
}

function shouldCheckPackage(packageName: string): boolean {
  if (requestedPackageNames.size === 0) {
    return true;
  }

  return requestedPackageNames.has(packageName);
}

function isPinnedSpec(currentSpec: string): boolean {
  return semver.valid(currentSpec) !== null;
}

async function collectUpdateCandidates(
  catalog: Record<string, string>,
): Promise<UpdateCandidate[]> {
  const candidates: UpdateCandidate[] = [];

  for (const [packageName, currentSpec] of Object.entries(catalog)) {
    if (!shouldCheckPackage(packageName)) {
      continue;
    }

    if (isPinnedSpec(currentSpec) && !options.includePinned) {
      continue;
    }

    if (semver.validRange(currentSpec) === null) {
      console.warn(`Skipping ${packageName}: invalid range "${currentSpec}".`);
      continue;
    }

    const versions = await fetchPackageVersions(packageName);
    const nextVersion = resolveNextVersion(currentSpec, versions);

    if (nextVersion === null) {
      console.warn(`Skipping ${packageName}: no compatible version found.`);
      continue;
    }

    const nextSpec = preserveRangePrefix(currentSpec, nextVersion);

    if (nextSpec === currentSpec) {
      continue;
    }

    candidates.push({ packageName, currentSpec, nextSpec });
  }

  return candidates;
}

async function runInstall(): Promise<boolean> {
  const process = Bun.spawn(installCommand, {
    stdout: "inherit",
    stderr: "inherit",
  });

  const exitCode = await process.exited;

  return exitCode === 0;
}

async function restoreFiles(
  packageJsonBackup: string,
  lockfileBackup: string | null,
): Promise<void> {
  await Bun.write(packageJsonPath, packageJsonBackup);

  if (lockfileBackup !== null) {
    await Bun.write(lockfilePath, lockfileBackup);
  }
}

async function applyCandidate(candidate: UpdateCandidate): Promise<boolean> {
  const packageJsonBackup = await Bun.file(packageJsonPath).text();
  const lockfileBackup = await readTextIfPresent(lockfilePath);
  const packageJson = await readPackageJson();
  const catalog = getCatalog(packageJson);

  catalog[candidate.packageName] = candidate.nextSpec;
  await writePackageJson(packageJson);

  if (options.skipInstall) {
    return true;
  }

  const installPassed = await runInstall();

  if (installPassed) {
    return true;
  }

  await restoreFiles(packageJsonBackup, lockfileBackup);
  return false;
}

async function main(): Promise<void> {
  const packageJson = await readPackageJson();
  const catalog = getCatalog(packageJson);
  const candidates = await collectUpdateCandidates(catalog);

  if (candidates.length === 0) {
    console.info("Catalog is already up to date.");
    return;
  }

  if (options.dryRun) {
    for (const candidate of candidates) {
      console.info(
        `${candidate.packageName}: ${candidate.currentSpec} -> ${candidate.nextSpec}`,
      );
    }

    return;
  }

  for (const candidate of candidates) {
    console.info(
      `Updating ${candidate.packageName}: ${candidate.currentSpec} -> ${candidate.nextSpec}`,
    );

    const updated = await applyCandidate(candidate);

    if (!updated) {
      console.warn(`Rolled back ${candidate.packageName}.`);
    }
  }
}

await main();
