#!/usr/bin/env bash

# Copyright (c) 2026 VinVel
# 
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, version 3 only.
#  
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.
# 
# Project home: hyperion.velcore.net

set -euo pipefail

prompt_required() {
  local var_name="$1"
  local default_value="$2"
  local input_value

  if [[ -n "$default_value" ]]; then
    read -r -p "$var_name [$default_value]: " input_value
    input_value="${input_value:-$default_value}"
  else
    read -r -p "$var_name: " input_value
  fi

  input_value="${input_value//\\//}"

  if [[ -z "$input_value" ]]; then
    echo "$var_name is required." >&2
    exit 1
  fi

  printf -v "$var_name" '%s' "$input_value"
}

prompt_min_sdk() {
  local default_value="$1"
  local input_value

  read -r -p "ANDROID_MIN_SDK [$default_value]: " input_value
  input_value="${input_value:-$default_value}"

  if [[ ! "$input_value" =~ ^[0-9]+$ ]]; then
    echo "ANDROID_MIN_SDK must be a positive integer." >&2
    exit 1
  fi

  ANDROID_MIN_SDK="$input_value"
}

detect_latest_ndk_home() {
  local android_home="$1"
  local ndk_root
  ndk_root="${android_home//\\//}/ndk"

  if [[ ! -d "$ndk_root" ]]; then
    echo ""
    return
  fi

  local versions=()
  while IFS= read -r version_dir; do
    versions+=("$version_dir")
  done < <(
    find "$ndk_root" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' \
      | grep -E '^[0-9]+([.][0-9]+)*$' \
      | sort -V
  )

  if [[ ${#versions[@]} -eq 0 ]]; then
    echo ""
    return
  fi

  local latest="${versions[${#versions[@]}-1]}"
  echo "$ndk_root/$latest"
}

echo "Android .env generator (press Enter to accept defaults)"

default_android_home="${ANDROID_HOME:-}"
prompt_required ANDROID_HOME "$default_android_home"

detected_ndk_home="$(detect_latest_ndk_home "$ANDROID_HOME")"
if [[ -n "$detected_ndk_home" ]]; then
  echo "Detected latest NDK: $detected_ndk_home"
fi

default_ndk_home="${NDK_HOME:-$detected_ndk_home}"
prompt_required NDK_HOME "$default_ndk_home"

prompt_min_sdk "${ANDROID_MIN_SDK:-24}"

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
env_path="$repo_root/.android-env"

cat > "$env_path" <<EOF
ANDROID_HOME=$ANDROID_HOME
NDK_HOME=$NDK_HOME
ANDROID_MIN_SDK=$ANDROID_MIN_SDK

# Cargo target linker/ar overrides
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android\$ANDROID_MIN_SDK-clang
CARGO_TARGET_AARCH64_LINUX_ANDROID_AR=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar

CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi\$ANDROID_MIN_SDK-clang
CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_AR=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar

CARGO_TARGET_I686_LINUX_ANDROID_LINKER=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android\$ANDROID_MIN_SDK-clang
CARGO_TARGET_I686_LINUX_ANDROID_AR=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar

CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android\$ANDROID_MIN_SDK-clang
CARGO_TARGET_X86_64_LINUX_ANDROID_AR=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar

# cc-rs variables for crates with native C/C++ build steps (ring, blake3, etc.)
CC_aarch64_linux_android=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android\$ANDROID_MIN_SDK-clang
CXX_aarch64_linux_android=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android\$ANDROID_MIN_SDK-clang++
AR_aarch64_linux_android=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar

CC_armv7_linux_androideabi=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi\$ANDROID_MIN_SDK-clang
CXX_armv7_linux_androideabi=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi\$ANDROID_MIN_SDK-clang++
AR_armv7_linux_androideabi=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar

CC_i686_linux_android=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android\$ANDROID_MIN_SDK-clang
CXX_i686_linux_android=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android\$ANDROID_MIN_SDK-clang++
AR_i686_linux_android=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar

CC_x86_64_linux_android=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android\$ANDROID_MIN_SDK-clang
CXX_x86_64_linux_android=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android\$ANDROID_MIN_SDK-clang++
AR_x86_64_linux_android=\$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar
EOF

echo "Generated $env_path"
