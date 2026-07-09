[![CI](https://github.com/VinVel/Hyperion/actions/workflows/ci.yml/badge.svg)](https://github.com/VinVel/Hyperion/actions/workflows/ci.yml)

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
  lld \
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
  lld \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  libxdo-devel
sudo dnf group install "c-development"
```
