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
