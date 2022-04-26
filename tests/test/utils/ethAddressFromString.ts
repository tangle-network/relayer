/*
 * Copyright 2022 Webb Technologies Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
import { u8aToHex } from '@polkadot/util';
// resize byte array to given new size, If array is small fill rest with defaultvalue.
function resize(
  arr: number[],
  newSize: number,
  defaultValue: number
): number[] {
  return [
    ...arr,
    ...Array(Math.max(newSize - arr.length, 0)).fill(defaultValue),
  ];
}
// convert string to byte array
function getBytes(str: string) {
  return [...Buffer.from(str)];
}
// generate 20 byte ethereum type address from string.
export function ethAddressFromString(str: string): `0x${string}` {
  const bytes = new Uint8Array(resize(getBytes(str), 20, 0));
  const address = u8aToHex(bytes);
  return address as `0x${string}`;
}
