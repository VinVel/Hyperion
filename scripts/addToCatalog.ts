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

type DependencyMap = Record<string, string>;

type PackageJson = {
  workspaces?: {
    catalog?: Record<string, string>;
  };

  dependencies?: DependencyMap;
  devDependencies?: DependencyMap;
};

type RegistryMetadata = {
  "dist-tags"?: {
    latest?: string;
  };

  versions?: Record<string, unknown>;
};

type AddCandidate = {
  packageName: string;
  versionSpec: string;
};

const packageJsonPath = "package.json";
const lockfilePath = "bun.lock";
const npmRegistryUrl = "https://registry.npmjs.org";

const installCommand = ["bun", "install"];

const options = {
  dryRun: Bun.argv.includes("--dry-run"),
  dev: Bun.argv.includes("--dev"),
  exact: Bun.argv.includes("--exact"),
  skipInstall: Bun.argv.includes("--skip-install"),
};

const requestedPackageNames = Bun.argv
  .slice(2)
  .filter((argument) => !argument.startsWith("--"));

if (requestedPackageNames.length === 0) {
  throw new Error("No package names provided.");
}

function readPackageJson(): Promise<PackageJson> {
  return Bun.file(packageJsonPath).json() as Promise<PackageJson>;
}

async function writePackageJson(packageJson: PackageJson): Promise<void> {
  await Bun.write(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`);
}

async function readTextIfPresent(path: string): Promise<string | null> {
  const file = Bun.file(path);

  if (!(await file.exists())) {
    return null;
  }

  return file.text();
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

async function fetchLatestVersion(packageName: string): Promise<string> {
  const response = await fetch(packageRegistryUrl(packageName));

  if (!response.ok) {
    throw new Error(
      `Could not fetch ${packageName}: ${response.status} ${response.statusText}`,
    );
  }

  const metadata = (await response.json()) as RegistryMetadata;

  const latestTag = metadata["dist-tags"]?.latest;

  if (latestTag !== undefined && semver.valid(latestTag) !== null) {
    return latestTag;
  }

  const versions = Object.keys(metadata.versions ?? {})
    .filter((version) => semver.valid(version) !== null)
    .sort(semver.rcompare);

  const latestVersion = versions[0];

  if (latestVersion === undefined) {
    throw new Error(`No valid versions found for ${packageName}.`);
  }

  return latestVersion;
}

function createVersionSpec(version: string): string {
  if (options.exact) {
    return version;
  }

  return `^${version}`;
}

async function collectCandidates(): Promise<AddCandidate[]> {
  const candidates: AddCandidate[] = [];

  for (const packageName of requestedPackageNames) {
    const latestVersion = await fetchLatestVersion(packageName);

    candidates.push({
      packageName,
      versionSpec: createVersionSpec(latestVersion),
    });
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

function ensureDependencySection(
  packageJson: PackageJson,
  dev: boolean,
): DependencyMap {
  if (dev) {
    packageJson.devDependencies ??= {};
    return packageJson.devDependencies;
  }

  packageJson.dependencies ??= {};
  return packageJson.dependencies;
}

async function applyCandidate(candidate: AddCandidate): Promise<boolean> {
  const packageJsonBackup = await Bun.file(packageJsonPath).text();
  const lockfileBackup = await readTextIfPresent(lockfilePath);

  const packageJson = await readPackageJson();
  const catalog = getCatalog(packageJson);

  catalog[candidate.packageName] = candidate.versionSpec;

  const dependencySection = ensureDependencySection(packageJson, options.dev);

  dependencySection[candidate.packageName] = "catalog:";

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

  const candidates = await collectCandidates();

  for (const candidate of candidates) {
    if (catalog[candidate.packageName] !== undefined) {
      console.warn(
        `Skipping ${candidate.packageName}: already exists in catalog.`,
      );

      continue;
    }

    if (options.dryRun) {
      console.info(
        `Would add ${candidate.packageName}: ${candidate.versionSpec}`,
      );

      continue;
    }

    console.info(`Adding ${candidate.packageName}: ${candidate.versionSpec}`);

    const added = await applyCandidate(candidate);

    if (!added) {
      console.warn(`Rolled back ${candidate.packageName}.`);
    }
  }
}

await main();
