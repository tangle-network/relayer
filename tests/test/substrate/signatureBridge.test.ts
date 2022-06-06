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
// This our basic Substrate Anchor Transaction Relayer Tests.
// These are for testing mocked proposal signing backend for substrate

import '@webb-tools/types';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import isCi from 'is-ci';
import { WebbRelayer, Pallet } from '../../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../../lib/localProtocolSubstrate.js';
import { hexToU8a } from '@polkadot/util';
import {
  UsageMode,
  defaultEventsWatcherValue,
} from '../../lib/substrateNodeBase.js';
import { ApiPromise, Keyring } from '@polkadot/api';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { BigNumber } from 'ethers';
import { Note, NoteGenInput } from '@webb-tools/sdk-core';

describe('Signature Bridge <> Mocked Proposal Signing Backend', function () {
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;

  // Governer key
  const PK1 =
    '0x9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60';
  const uncompressedKey =
    '8db55b05db86c0b1786ca49f095d76344c9e6056b2f02701a7e7f3c20aabfd913ebbe148dd17c56551a52952371071a6c604b3f3abe8f2c8fa742158ea6dd7d4';

  let webbRelayer: WebbRelayer;

  before(async () => {
    const usageMode: UsageMode = isCi
      ? { mode: 'docker', forcePullImage: false }
      : {
          mode: 'host',
          nodePath: path.resolve(
            '../../protocol-substrate/target/release/webb-standalone-node'
          ),
        };

    // enable pallets
    const enabledPallets: Pallet[] = [
      {
        pallet: 'AnchorBn254',
        eventsWatcher: defaultEventsWatcherValue,
      },
      {
        pallet: 'SignatureBridge',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];

    aliceNode = await LocalProtocolSubstrate.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enabledPallets,
      enableLogging: false,
    });

    bobNode = await LocalProtocolSubstrate.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode,
      ports: 'auto',
    });

    // set proposal signing backend and linked anchors
    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
      linkedAnchors: [
        {
          chain: 1080,
          tree: 5,
        },
      ],
    });

    // Wait until we are ready and connected
    const api = await aliceNode.api();
    await api.isReady;

    //force set maintainer
    let setMaintainerCall = api.tx.signatureBridge!.forceSetMaintainer!(
      Array.from(hexToU8a(uncompressedKey))
    );
    // execute sudo transaction.
    await aliceNode.sudoExecuteTransaction(setMaintainerCall);

    //whitelist chain
    let whitelistChainCall = api.tx.signatureBridge!.whitelistChain!(1080);
    // execute sudo transaction.
    await aliceNode.sudoExecuteTransaction(whitelistChainCall);

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(8000, 8888) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
      showLogs: false,
    });
    await webbRelayer.waitUntilReady();
  });

  it('Simple Anchor Deposit', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Dave');
    const note = await makeDeposit(api, aliceNode, account);
    const typedSourceChainId = 2199023255553;

    // now we wait for the proposal to be signed by mocked backend and then send data to signature bridge
    await webbRelayer.waitForEvent({
      kind: 'signing_backend',
      event: {
        backend: 'Mocked',
      },
    });

    // now we wait for the proposals to verified and executed by signature bridge

    await webbRelayer.waitForEvent({
      kind: 'signature_bridge',
      event: {
        call: 'execute_proposal_with_signature',
      },
    });
  });

  after(async () => {
    await aliceNode?.stop();
    await bobNode?.stop();
    await webbRelayer?.stop();
  });
});

// Helper methods, we can move them somewhere if we end up using them again.

function currencyToUnitI128(currencyAmount: number) {
  let bn = BigNumber.from(currencyAmount);
  return bn.mul(1_000_000_000_000);
}

async function createAnchor(
  api: ApiPromise,
  aliceNode: LocalProtocolSubstrate,
  typedSourceChainId: number
) {
  // set resource ID
  let resourceId = new Uint8Array([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    5, 2, 0, 0, 0, 4, 56,
  ]);
  let resource = [...Buffer.from('test')];

  //create anchor and resource to anchor
  // create(depositSize, srcId, resourceId, maxEdges, depth, assetId)
  let createAnchorCall = api.tx.anchorHandlerBn254!
    .executeAnchorCreateProposal!(
    currencyToUnitI128(100).toString(),
    typedSourceChainId,
    resourceId,
    10,
    3,
    0
  );
  await aliceNode.sudoExecuteTransaction(createAnchorCall);
}

async function createAnchorDepositTx(api: ApiPromise): Promise<{
  tx: SubmittableExtrinsic<'promise'>;
  note: Note;
}> {
  const noteInput: NoteGenInput = {
    protocol: 'anchor',
    version: 'v2',
    sourceChain: '2199023256632',
    targetChain: '2199023256632',
    sourceIdentifyingData: `5`,
    targetIdentifyingData: `5`,
    tokenSymbol: 'WEBB',
    amount: '1',
    denomination: '18',
    backend: 'Arkworks',
    hashFunction: 'Poseidon',
    curve: 'Bn254',
    width: '4',
    exponentiation: '5',
  };
  const note = await Note.generateNote(noteInput);
  // @ts-ignore
  const treeIds = await api.query.anchorBn254.anchors.keys();
  const sorted = treeIds.map((id) => Number(id.toHuman())).sort();
  const treeId = sorted[0] || 5;
  const leaf = note.getLeaf();
  // @ts-ignore
  const tx = api.tx.anchorBn254.deposit(treeId, leaf);
  return { tx, note };
}

function createAccount(accountId: string): any {
  const keyring = new Keyring({ type: 'sr25519' });
  const account = keyring.addFromUri(accountId);

  return account;
}

async function makeDeposit(
  api: any,
  aliceNode: any,
  account: any
): Promise<Note> {
  const { tx, note } = await createAnchorDepositTx(api);

  // send the deposit transaction.
  const txSigned = await tx.signAsync(account);

  await aliceNode.executeTransaction(txSigned);

  return note;
}
