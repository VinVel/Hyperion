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

## Building & Developement

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

For Tauri desktop builds on Debian-based Linux systems, install the native
libraries required by GTK, GLib, and WebKit before running `pnpm tauri dev` or `pnpm tauri build`:

```bash
sudo apt update
sudo apt install \
  libglib2.0-dev \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```
