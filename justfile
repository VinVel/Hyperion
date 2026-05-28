#
# Copyright (c) 2026 VinVel
#
# SPDX-License-Identifier: AGPL-3.0-only
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, version 3 only.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.
#
# Project home: hyperion.velcore.net
#

set shell := ["bash", "-uc"]
set windows-shell := ["pwsh.exe", "-NoProfile", "-Command"]

# Rust
test-rust:
    cargo test

check-rust:
    cargo check
    cargo clippy --all-targets --all-features
    cargo fmt --check

fmt-rust:
    cargo fmt

# TypeScript
test-typescript:
    bun test

check-typescript:
    bun check
    bun lint

fmt-typescript:
    bun format

# Build Commands
build-desktop-debug:
    bun tauri build --verbose --no-bundle --debug -- --verbose

build-android-debug:
    bun tauri android build --verbose --debug --apk --split-per-abi --target aarch64 -- --verbose

# Composite Tasks
test: test-rust test-typescript
check: check-rust check-typescript
fmt: fmt-rust fmt-typescript
build-debug: build-desktop-debug build-android-debug
