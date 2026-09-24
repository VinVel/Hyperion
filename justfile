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

# Composite Tasks
test: test-rust
check: check-rust
fmt: fmt-rust 
