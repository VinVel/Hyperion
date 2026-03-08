<#
Copyright (c) 2026 VinVel

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as
published by the Free Software Foundation, version 3 only.
 
You should have received a copy of the GNU Affero General Public License
along with this program. If not, see <https://www.gnu.org/licenses/>.

Project home: hyperion.velcore.net
#>

$ErrorActionPreference = 'Stop'

function Read-RequiredValue([string]$Name, [string]$Default = '') {
  $prompt = if ($Default) { "$Name [$Default]" } else { $Name }
  $raw = Read-Host $prompt
  $value = if ([string]::IsNullOrWhiteSpace($raw)) { $Default } else { $raw }
  if ([string]::IsNullOrWhiteSpace($value)) {
    throw "$Name is required."
  }

  return ($value.Trim() -replace '\\', '/')
}

function Read-MinSdk([string]$Default = '24') {
  $raw = Read-Host "ANDROID_MIN_SDK [$Default]"
  $value = if ([string]::IsNullOrWhiteSpace($raw)) { $Default } else { $raw.Trim() }
  if ($value -notmatch '^[0-9]+$') {
    throw 'ANDROID_MIN_SDK must be a positive integer.'
  }

  return $value
}

function Get-HighestNdkHome([string]$AndroidHome) {
  if ([string]::IsNullOrWhiteSpace($AndroidHome)) {
    return ''
  }

  $androidHomeWin = $AndroidHome -replace '/', '\'
  $ndkRoot = Join-Path $androidHomeWin 'ndk'
  if (-not (Test-Path $ndkRoot)) {
    return ''
  }

  $candidate = Get-ChildItem -Path $ndkRoot -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^\d+(\.\d+)*$' } |
    Sort-Object { [version]$_.Name } -Descending |
    Select-Object -First 1

  if ($null -eq $candidate) {
    return ''
  }

  return ($candidate.FullName -replace '\\', '/')
}

Write-Host 'Android .env generator (press Enter to accept defaults)'

$defaultAndroidHome = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { '' }
$ANDROID_HOME = Read-RequiredValue 'ANDROID_HOME' $defaultAndroidHome

$detectedNdkHome = Get-HighestNdkHome $ANDROID_HOME
if ($detectedNdkHome) {
  Write-Host "Detected latest NDK: $detectedNdkHome"
}

$defaultNdkHome = if ($env:NDK_HOME) { $env:NDK_HOME } elseif ($detectedNdkHome) { $detectedNdkHome } else { '' }
$NDK_HOME = Read-RequiredValue 'NDK_HOME' $defaultNdkHome

$defaultMinSdk = if ($env:ANDROID_MIN_SDK) { $env:ANDROID_MIN_SDK } else { '24' }
$ANDROID_MIN_SDK = Read-MinSdk $defaultMinSdk

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$envPath = Join-Path $repoRoot '.android-env'

$content = @"
ANDROID_HOME=$ANDROID_HOME
NDK_HOME=$NDK_HOME
ANDROID_MIN_SDK=$ANDROID_MIN_SDK

# Cargo target linker/ar overrides
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/aarch64-linux-android`$ANDROID_MIN_SDK-clang.cmd
CARGO_TARGET_AARCH64_LINUX_ANDROID_AR=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-ar.exe

CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/armv7a-linux-androideabi`$ANDROID_MIN_SDK-clang.cmd
CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_AR=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-ar.exe

CARGO_TARGET_I686_LINUX_ANDROID_LINKER=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/i686-linux-android`$ANDROID_MIN_SDK-clang.cmd
CARGO_TARGET_I686_LINUX_ANDROID_AR=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-ar.exe

CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/x86_64-linux-android`$ANDROID_MIN_SDK-clang.cmd
CARGO_TARGET_X86_64_LINUX_ANDROID_AR=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-ar.exe

# cc-rs variables for crates with native C/C++ build steps (ring, blake3, etc.)
CC_aarch64_linux_android=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/aarch64-linux-android`$ANDROID_MIN_SDK-clang.cmd
CXX_aarch64_linux_android=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/aarch64-linux-android`$ANDROID_MIN_SDK-clang++.cmd
AR_aarch64_linux_android=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-ar.exe

CC_armv7_linux_androideabi=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/armv7a-linux-androideabi`$ANDROID_MIN_SDK-clang.cmd
CXX_armv7_linux_androideabi=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/armv7a-linux-androideabi`$ANDROID_MIN_SDK-clang++.cmd
AR_armv7_linux_androideabi=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-ar.exe

CC_i686_linux_android=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/i686-linux-android`$ANDROID_MIN_SDK-clang.cmd
CXX_i686_linux_android=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/i686-linux-android`$ANDROID_MIN_SDK-clang++.cmd
AR_i686_linux_android=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-ar.exe

CC_x86_64_linux_android=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/x86_64-linux-android`$ANDROID_MIN_SDK-clang.cmd
CXX_x86_64_linux_android=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/x86_64-linux-android`$ANDROID_MIN_SDK-clang++.cmd
AR_x86_64_linux_android=`$NDK_HOME/toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-ar.exe
"@

Set-Content -Path $envPath -Value $content -NoNewline
Write-Host "Generated $envPath"
