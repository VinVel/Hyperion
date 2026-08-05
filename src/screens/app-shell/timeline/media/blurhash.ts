/*
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

// Matrix blurhash placeholders are decoded to a tiny bitmap and stretched
// behind the actual thumbnail, keeping decode and paint work bounded.
const blurhashBitmapSize = 10;
const blurhashAlpha = 230;
const maximumBlurhashCacheItems = 128;
const blurhashBase83Alphabet =
  "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

const blurhashDataUrlCache = new Map<string, string>();

export function blurhashDataUrl(blurhash: string): string {
  const cachedDataUrl = blurhashDataUrlCache.get(blurhash);
  if (cachedDataUrl !== undefined) {
    return cachedDataUrl;
  }

  const decodedPixels = decodeBlurhash(
    blurhash,
    blurhashBitmapSize,
    blurhashBitmapSize,
  );
  if (!decodedPixels) {
    rememberBlurhashDataUrl(blurhash, "");
    return "";
  }

  const canvas = document.createElement("canvas");
  canvas.width = blurhashBitmapSize;
  canvas.height = blurhashBitmapSize;
  const context = canvas.getContext("2d");
  if (!context) {
    return "";
  }

  const imageData = context.createImageData(
    blurhashBitmapSize,
    blurhashBitmapSize,
  );
  imageData.data.set(decodedPixels);
  context.putImageData(imageData, 0, 0);
  const dataUrl = canvas.toDataURL("image/png");
  rememberBlurhashDataUrl(blurhash, dataUrl);
  return dataUrl;
}

function rememberBlurhashDataUrl(blurhash: string, dataUrl: string) {
  blurhashDataUrlCache.set(blurhash, dataUrl);
  while (blurhashDataUrlCache.size > maximumBlurhashCacheItems) {
    const oldestBlurhash = blurhashDataUrlCache.keys().next().value;
    if (!oldestBlurhash) {
      return;
    }
    blurhashDataUrlCache.delete(oldestBlurhash);
  }
}

function decodeBlurhash(
  blurhash: string,
  width: number,
  height: number,
): Uint8ClampedArray | null {
  if (blurhash.length < 6) {
    return null;
  }

  const sizeFlag = decode83(blurhash[0]);
  const componentX = (sizeFlag % 9) + 1;
  const componentY = Math.floor(sizeFlag / 9) + 1;
  const expectedLength = 4 + 2 * componentX * componentY;
  if (blurhash.length !== expectedLength) {
    return null;
  }

  const quantizedMaximumValue = decode83(blurhash[1]);
  const maximumValue = (quantizedMaximumValue + 1) / 166;
  const colors: Array<[number, number, number]> = [];
  for (let index = 0; index < componentX * componentY; index += 1) {
    if (index === 0) {
      colors.push(decodeDc(decode83(blurhash.slice(2, 6))));
      continue;
    }

    colors.push(
      decodeAc(
        decode83(blurhash.slice(4 + index * 2, 6 + index * 2)),
        maximumValue,
      ),
    );
  }

  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      let red = 0;
      let green = 0;
      let blue = 0;
      for (
        let componentYIndex = 0;
        componentYIndex < componentY;
        componentYIndex += 1
      ) {
        for (
          let componentXIndex = 0;
          componentXIndex < componentX;
          componentXIndex += 1
        ) {
          const basis =
            Math.cos((Math.PI * x * componentXIndex) / width) *
            Math.cos((Math.PI * y * componentYIndex) / height);
          const color = colors[componentXIndex + componentYIndex * componentX];
          red += color[0] * basis;
          green += color[1] * basis;
          blue += color[2] * basis;
        }
      }

      const pixelOffset = 4 * (x + y * width);
      pixels[pixelOffset] = linearToSrgb(red);
      pixels[pixelOffset + 1] = linearToSrgb(green);
      pixels[pixelOffset + 2] = linearToSrgb(blue);
      pixels[pixelOffset + 3] = blurhashAlpha;
    }
  }

  return pixels;
}

function decode83(value: string): number {
  let result = 0;
  for (const character of value) {
    const digit = blurhashBase83Alphabet.indexOf(character);
    if (digit < 0) {
      return 0;
    }
    result = result * 83 + digit;
  }
  return result;
}

function decodeDc(value: number): [number, number, number] {
  return [
    srgbToLinear((value >> 16) & 255),
    srgbToLinear((value >> 8) & 255),
    srgbToLinear(value & 255),
  ];
}

function decodeAc(
  value: number,
  maximumValue: number,
): [number, number, number] {
  const quantizedRed = Math.floor(value / (19 * 19));
  const quantizedGreen = Math.floor(value / 19) % 19;
  const quantizedBlue = value % 19;

  return [
    signedPower((quantizedRed - 9) / 9, 2) * maximumValue,
    signedPower((quantizedGreen - 9) / 9, 2) * maximumValue,
    signedPower((quantizedBlue - 9) / 9, 2) * maximumValue,
  ];
}

function signedPower(value: number, exponent: number): number {
  return Math.sign(value) * Math.abs(value) ** exponent;
}

function srgbToLinear(value: number): number {
  const scaledValue = value / 255;
  if (scaledValue <= 0.04045) {
    return scaledValue / 12.92;
  }
  return ((scaledValue + 0.055) / 1.055) ** 2.4;
}

function linearToSrgb(value: number): number {
  const clampedValue = Math.min(1, Math.max(0, value));
  if (clampedValue <= 0.0031308) {
    return Math.round(clampedValue * 12.92 * 255);
  }
  return Math.round((1.055 * clampedValue ** (1 / 2.4) - 0.055) * 255);
}
