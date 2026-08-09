[![CI](https://github.com/VinVel/Hyperion/actions/workflows/ci.yml/badge.svg)](https://github.com/VinVel/Hyperion/actions/workflows/ci.yml)

> [!NOTE]  
> This repository makes heavy use of AI, especially for UI components. While I try to make sure everything works through CI, strict Clippy rules, extensive testing and common sense. I won't be able to catch all types of errors that appear when you use agentic coding. If you feel uncomfortable using such type of software I'd recommend not using it.

> [!IMPORTANT]
> Hyperion is pre-alpha. Nothing is stable, not even the master branch. If you somehow stumbled upon this repository, good luck!

## [LICENSE](./LICENSE)
```
    Hyperion, a multiplattform Matrix Client
    Copyright (C) 2026 VinVel

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as
    published by the Free Software Foundation, only version 3 of the
    License.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
```
## Feature List: For a full list of Features check out the [Features](./FEATURES.md) list.

## Building & Development

Runtime logging, privacy rules, `RUST_LOG`, and native platform query commands
are documented in [Native diagnostics](./docs/debugging.md).

### Android setup

For a reproducible local setup set the environment variable `ANDROID_HOME` and it should just work when executing the following command:
```bash
pnpm tauri android build
```
or
```bash
pnpm tauri android dev
```

### Linux desktop setup

For Tauri desktop builds on Linux, install the native libraries required by the
current CEF-backed GTK runtime before running `pnpm tauri dev` or
`pnpm tauri build`.

On Debian or Ubuntu:

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  mold \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

On Fedora:

```bash
sudo dnf check-update
sudo dnf install webkit2gtk4.1-devel \
  openssl-devel \
  mold \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  libxdo-devel
sudo dnf group install "c-development"
```
