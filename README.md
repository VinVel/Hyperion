[![Android CI](https://github.com/VinVel/Hyperion/actions/workflows/android-ci.yml/badge.svg)](https://github.com/VinVel/Hyperion/actions/workflows/android-ci.yml)

[![iOS CI](https://github.com/VinVel/Hyperion/actions/workflows/ios-simulator-ci.yml/badge.svg)](https://github.com/VinVel/Hyperion/actions/workflows/ios-simulator-ci.yml)

[![Desktop CI](https://github.com/VinVel/Hyperion/actions/workflows/desktop-ci.yml/badge.svg)](https://github.com/VinVel/Hyperion/actions/workflows/desktop-ci.yml)

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

## Android setup

For a reproducible local setup, use the helper scripts in `scripts/`.
They generate two ignored files:

- `.cargo/config.toml` with Android linkers and `cc-rs` environment variables
- `src-tauri/gen/android/local.properties` with `sdk.dir`

### Windows

```powershell
.\scripts\generate-android-env.ps1
```

### Linux

```bash
./scripts/generate-android-env.sh
```

The scripts use `ANDROID_HOME` when it is already set and otherwise expect an
installed Android SDK that contains an `ndk/<version>` directory.

## Linux desktop setup

For Tauri desktop builds on Debian-based Linux systems, install the native
libraries required by GTK, GLib, and WebKit before running `bun tauri dev` or `bun tauri build`:

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