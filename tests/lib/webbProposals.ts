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
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { ChainType, ResourceId, toFixedHex } from '@webb-tools/sdk-core';

/**
 * A TargetSystem is 26 bytes hex encoded string of the following format
 * PalletIndex: 1byte
 * TreeId: 4 bytes
 * we pad it with zero to make it 26 bytes
 */
export function makeSubstrateTargetSystem(
  treeId: number,
  palletIndex: string
): string {
  const rId = new Uint8Array(20);
  const index = hexToU8a(palletIndex).slice(0, 1);
  const treeBytes = hexToU8a(toFixedHex(treeId, 4));
  rId.set(index, 15); // 15-16
  rId.set(treeBytes, 16); // 16-20
  return u8aToHex(rId);
}

export function createSubstrateResourceId(
  chainId: number,
  treeId: number,
  palletIndex: string
): ResourceId {
  const substrateTargetSystem = makeSubstrateTargetSystem(treeId, palletIndex);
  // set resource ID
  const resourceId = new ResourceId(
    toFixedHex(substrateTargetSystem, 20),
    ChainType.Substrate,
    chainId
  );
  return resourceId;
}
