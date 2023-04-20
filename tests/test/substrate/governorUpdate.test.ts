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
import '@webb-tools/dkg-substrate-types';
import { expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import isCi from 'is-ci';
import { WebbRelayer, Pallet } from '../../lib/webbRelayer.js';
import { LocalTangle } from '../../lib/localTangle.js';
import { u8aToHex } from '@polkadot/util';
import { UsageMode } from '@webb-tools/test-utils';
import { defaultEventsWatcherValue } from '../../lib/utils.js';
import { timeout } from '../../lib/timeout.js';

describe.skip('Substrate SignatureBridge Governor Update', function () {
  const tmpDirPath = temp.mkdirSync();
  // Tangle nodes
  let aliceNode: LocalTangle;
  let charlieNode: LocalTangle;

  let webbRelayer: WebbRelayer;

  before(async () => {
    const usageMode: UsageMode = isCi
      ? { mode: 'docker', forcePullImage: false }
      : {
          mode: 'host',
          nodePath: path.resolve(
            '../../tangle/target/release/tangle-standalone'
          ),
        };
    const enabledPallets: Pallet[] = [
      {
        pallet: 'SignatureBridge',
        eventsWatcher: defaultEventsWatcherValue,
      },
      {
        pallet: 'DKGProposalHandler',
        eventsWatcher: defaultEventsWatcherValue,
      },
      {
        pallet: 'DKG',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];

    // Step 1. We start tangle nodes.
    aliceNode = await LocalTangle.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });

    charlieNode = await LocalTangle.start({
      name: 'substrate-charlie',
      authority: 'charlie',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });
    // Wait until we are ready and connected
    const api = await aliceNode.api();
    await api.isReady;
    console.log('substrate node ready');
    const chainId = await aliceNode.getChainId();

    // Step 2. We need to wait until the public key is on chain.
    await aliceNode.waitForEvent({
      section: 'dkg',
      method: 'PublicKeySignatureChanged',
    });

    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      chainId: chainId,
      proposalSigningBackend: { type: 'DKGNode', chainId },
      enabledPallets,
    });

    // Step 4. We force set maintainer on signature bridge.
    const dkgPublicKey = await aliceNode.fetchDkgPublicKey();
    expect(dkgPublicKey).to.not.be.null;
    const refreshNonce = await api.query.dkg.refreshNonce();

    // force set maintainer
    const setMaintainerCall = api.tx.signatureBridge.forceSetMaintainer(
      refreshNonce,
      dkgPublicKey!
    );
    await aliceNode.sudoExecuteTransaction(setMaintainerCall);

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(8000, 8888) });
    webbRelayer = new WebbRelayer({
      commonConfig: {
        port: relayerPort,
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
    });
    await webbRelayer.waitUntilReady();
  });

  it('ownership should be transfered when the DKG rotates', async () => {
    // Now we just need to force the DKG to rotate/refresh.
    const api = await aliceNode.api();
    const forceIncrementNonce = api.tx.dkg?.manualIncrementNonce?.();
    const forceRefresh = api.tx.dkg?.manualRefresh?.();
    await timeout(
      aliceNode.sudoExecuteTransaction(forceIncrementNonce),
      30_000
    );
    await timeout(aliceNode.sudoExecuteTransaction(forceRefresh), 60_000);
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
    const dkgPublicKey = await aliceNode.fetchDkgPublicKey();
    expect(dkgPublicKey).to.not.be.null;

    const maintainer = await api.query.signatureBridge.maintainer();
    const aliceMainatinerPubKey = u8aToHex(maintainer);
    expect(dkgPublicKey).to.eq(aliceMainatinerPubKey);
  });

  after(async () => {
    await aliceNode?.stop();
    await charlieNode?.stop();
    await webbRelayer?.stop();
  });
});
