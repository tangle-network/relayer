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
// This our basic Substrate VAnchor Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just to relay transactions for us.

import '@webb-tools/protocol-substrate-types';
import { assert, expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import isCi from 'is-ci';
import {
  WebbRelayer,
  Pallet,
} from '../../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../../lib/localProtocolSubstrate.js';
import { u8aToHex,  } from '@polkadot/util';
import { UsageMode } from '@webb-tools/test-utils';
import {
  defaultEventsWatcherValue,
} from '../../lib/utils.js';
import { LocalDkg } from 'lib/localDkg.js';
import { timeout } from 'lib/timeout.js';
import { ECPairAPI, TinySecp256k1Interface, ECPairFactory } from 'ecpair';
import * as TinySecp256k1 from 'tiny-secp256k1';
import { ethers } from 'ethers';

describe('Substrate SignatureBridge Governor Update', function () {
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;

  // dkg nodes
  let dkgNode1: LocalDkg;
  let dkgNode2: LocalDkg;
  let dkgNode3: LocalDkg;

  let webbRelayer: WebbRelayer;
  const PK1 = u8aToHex(ethers.utils.randomBytes(32));

  before(async () => {
    const usageMode: UsageMode = isCi
      ? { mode: 'docker', forcePullImage: false }
      : {
          mode: 'host',
          nodePath: path.resolve(
            '../../protocol-substrate/target/release/webb-standalone-node'
          ),
        };
    const enabledPallets: Pallet[] = [
      {
        pallet: 'VAnchorBn254',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];

    // Step 1. We initialize DKG nodes.
    dkgNode1 = await LocalDkg.start({
        name: 'dkg-alice',
        authority: 'alice',
        usageMode,
        ports: 'auto',
      });
  
    dkgNode2 = await LocalDkg.start({
        name: 'dkg-bob',
        authority: 'bob',
        usageMode,
        ports: 'auto',
      });
  
    dkgNode3 = await LocalDkg.start({
        name: 'dkg-charlie',
        authority: 'charlie',
        usageMode,
        ports: 'auto',
        enableLogging: false,
      });

    const dkgEnabledPallets: Pallet[] = [
    {
        pallet: 'DKGProposalHandler',
        eventsWatcher: defaultEventsWatcherValue,
    },
    {
        pallet: 'DKG',
        eventsWatcher: defaultEventsWatcherValue,
    },
    ];
    // Wait until we are ready and connected
    const dkgApi = await dkgNode3.api();
    await dkgApi.isReady;

    const dkgNodeChainId = await dkgNode3.getChainId();
   
    await dkgNode3.writeConfig(`${tmpDirPath}/${dkgNode3.name}.json`, {
    suri: '//Charlie',
    chainId: dkgNodeChainId,
    enabledPallets: dkgEnabledPallets,
    });

    // Step 2. We need to wait until the public key is on chain.
    await dkgNode3.waitForEvent({
    section: 'dkg',
    method: 'PublicKeySubmitted',
    });
  
    // Step 3. We start protocol-substrate nodes.
    aliceNode = await LocalProtocolSubstrate.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });

    bobNode = await LocalProtocolSubstrate.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });
    // Wait until we are ready and connected
    const api = await aliceNode.api();
    await api.isReady;

    const chainId = await aliceNode.getChainId();

    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      chainId: chainId,
      proposalSigningBackend: { type: 'DKGNode', node: dkgNode3.name },
      enabledPallets,
    });


    // Step 4. We force set maintainer on protocol-substrate node.
    const dkgPublicKey = await dkgNode3.fetchDkgPublicKey();
    expect(dkgPublicKey).to.not.be.null;
    const refreshNonce = await dkgApi.query.dkg.refreshNonce();

    // force set maintainer
    const setMaintainerCall = api.tx.signatureBridge.forceSetMaintainer(dkgPublicKey!);
    await aliceNode.sudoExecuteTransaction(setMaintainerCall);

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(8000, 8888) });
    webbRelayer = new WebbRelayer({
      commonConfig: {
        port: relayerPort,
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: false,
    });
    await webbRelayer.waitUntilReady();
  });

  it('ownership should be transfered when the DKG rotates', async () => {
     // Now we just need to force the DKG to rotate/refresh.
     const dkgApi = await dkgNode3.api();
     const forceIncrementNonce = dkgApi.tx.dkg.manualIncrementNonce();
     const forceRefresh = dkgApi.tx.dkg.manualRefresh();
     await timeout(
        dkgNode3.sudoExecuteTransaction(forceIncrementNonce),
       30_000
     );
     await timeout(dkgNode3.sudoExecuteTransaction(forceRefresh), 60_000);
     // Now we just need for the relayer to pick up the new DKG events.
     const chainId = await aliceNode.getChainId();
     await webbRelayer.waitForEvent({
        kind: 'tx_queue',
        event: {
          ty: 'SUBSTRATE',
          chain_id: chainId.toString(),
          finalized: true,
        },
      });

    // Now we need to check that the ownership was transfered.
    const dkgPublicKey = await dkgNode3.fetchDkgPublicKey();
    expect(dkgPublicKey).to.not.be.null;

    const api = await aliceNode.api();
    const maintainer = await api.query.signatureBridge.maintainer();
    const tinysecp: TinySecp256k1Interface = TinySecp256k1;
    const ECPair: ECPairAPI = ECPairFactory(tinysecp);
    const aliceMainatinerPubKey = ECPair.fromPublicKey(Buffer.from(maintainer),{
        compressed: false,
      }).publicKey.toString('hex');
    
    expect(dkgPublicKey).to.eq(aliceMainatinerPubKey);
  });

  after(async () => {
    await aliceNode?.stop();
    await bobNode?.stop();
    await dkgNode1?.stop();
    await dkgNode2?.stop();
    await dkgNode3?.stop();
    await webbRelayer?.stop();
  });
});

