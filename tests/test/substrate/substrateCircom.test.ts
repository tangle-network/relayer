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
// This our basic Substrate VAnchor Transaction Relayer Tests (Circom).
// These are for testing the basic relayer functionality. which is just to relay transactions for us.

import { expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import isCi from 'is-ci';
import {
  WebbRelayer,
  Pallet,
  SubstrateFeeInfo,
} from '../../lib/webbRelayer.js';
import { BigNumber, ethers } from 'ethers';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import { Utxo, calculateTypedChainId, ChainType } from '@webb-tools/sdk-core';
import { UsageMode } from '@webb-tools/test-utils';
import { createAccount, defaultEventsWatcherValue } from '../../lib/utils.js';
import {
  padUtxos,
  setupTransaction,
  vanchorDeposit,
} from '../../lib/substrateVAnchor.js';
import { LocalTangle } from '../../lib/localTangle.js';

describe('Substrate VAnchor Private Transaction Relayer Tests Using Circom', function () {
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalTangle;
  let charlieNode: LocalTangle;
  let webbRelayer: WebbRelayer;
  const PK1 = u8aToHex(ethers.utils.randomBytes(32));

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
        pallet: 'VAnchorBn254',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];

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

    const chainId = await aliceNode.getChainId();

    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      chainId: chainId,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
      enabledPallets,
    });

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

  it('should withdraw using private transaction', async () => {
    const api = await aliceNode.api();
    // 1. Create vanchor on Substrate chain with height 30 and maxEdges = 1
    const createVAnchorCall = api.tx.vAnchorBn254.create!(1, 30, 0);
    // execute sudo transaction.
    await aliceNode.sudoExecuteTransaction(createVAnchorCall);
    const nextTreeId = await api.query.merkleTreeBn254.nextTreeId();
    const treeId = nextTreeId.toNumber() - 1;

    // ChainId of the substrate chain
    const substrateChainId = await aliceNode.getChainId();
    const typedSourceChainId = calculateTypedChainId(
      ChainType.Substrate,
      substrateChainId
    );

    const typedTargetChainId = calculateTypedChainId(
      ChainType.Substrate,
      substrateChainId
    );
    const account = createAccount('//Dave');
    const amount = BigNumber.from(1000).mul(BigNumber.from(10).pow(18));

    // 2. Deposit amount on substrate chain.
    const data = await vanchorDeposit(
      account,
      treeId,
      typedSourceChainId.toString(), // source chain Id
      typedTargetChainId.toString(), // target chain Id
      amount, // public amount
      aliceNode
    );

    // 3. Now we withdraw it using the relayer.
    const relayerInfo = await webbRelayer.info();
    const relayerBeneficiary =
      relayerInfo.substrate[substrateChainId.toString()]!.beneficiary;
    expect(relayerBeneficiary).to.not.be.undefined;

    const inputUtxos = data.depositUtxos;
    const outputUtxos = await padUtxos(Number(typedSourceChainId), [], 2);

    const leavesMap = {};

    const address = account.address;
    // Initially leaves will be empty
    leavesMap[typedSourceChainId.toString()] = [];
    const tree = await api.query.merkleTreeBn254.trees(treeId);

    const vanchor = await api.query.vAnchorBn254.vAnchors(treeId);
    const asset = vanchor.unwrap().asset;
    const assetId = asset.toU8a();
    const root = tree.unwrap().root.toHex();
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    // @ts-ignore
    const neighborRoots: string[] = await api.rpc.lt
      .getNeighborRoots(treeId)
      .then((roots: any) => roots.toHuman());

    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    // @ts-ignore
    const leaves: string[] = await api.rpc.mt
      .getLeaves(treeId, 0, 256)
      .then((leaves: any) => leaves.toHuman());
    leavesMap[typedTargetChainId.toString()] = leaves;

    const rootsSet = [hexToU8a(root), hexToU8a(neighborRoots[0])];
    const recipientAddress = u8aToHex(decodeAddress(address));
    const relayerAddress = u8aToHex(decodeAddress(relayerBeneficiary));
    const dummyTx = await setupTransaction(
      Number(typedTargetChainId),
      inputUtxos,
      outputUtxos as [Utxo, Utxo],
      BigNumber.from(0), // fee
      BigNumber.from(0), // refund
      rootsSet,
      recipientAddress, // recipient address
      relayerAddress, // relayer address
      u8aToHex(assetId),
      leavesMap
    );

    console.log({ dummyTx });

    // estimate fee
    const estimatedFeeInfo = await api.tx.vAnchorBn254
      .transact(treeId, dummyTx.publicInputs, dummyTx.extData)
      .paymentInfo(account);

    const feeInfoResponse = await webbRelayer.getSubstrateFeeInfo(
      substrateChainId,
      estimatedFeeInfo.partialFee.toBn()
    );
    expect(feeInfoResponse.status).equal(200);
    const feeInfo = await (feeInfoResponse.json() as Promise<SubstrateFeeInfo>);
    const estimatedFee = BigInt(feeInfo.estimatedFee);
    const refund = BigInt(0);
    const feeTotal = estimatedFee + refund;
    // check that the fee is less than the amount
    expect(
      BigNumber.from(feeTotal).lt(amount),
      `Fee ${feeTotal} should be less than amount ${amount.toBigInt()} `
    );

    const actualTx = await setupTransaction(
      Number(typedTargetChainId),
      inputUtxos,
      outputUtxos as [Utxo, Utxo],
      BigNumber.from(feeTotal), // fee
      BigNumber.from(refund), // refund
      rootsSet,
      recipientAddress,
      relayerAddress,
      u8aToHex(assetId),
      leavesMap
    );
    console.log({ actualTx });

    const balanceBefore = await api.query.system.account(account.address);
    // now we withdraw using private transaction
    await webbRelayer.substrateVAnchorWithdraw(
      substrateChainId,
      treeId,
      actualTx.publicInputs,
      actualTx.extData
    );

    // now we wait for relayer to execute private transaction.

    await webbRelayer.waitForEvent({
      kind: 'private_tx',
      event: {
        ty: 'SUBSTRATE',
        chain_id: substrateChainId.toString(),
        finalized: true,
      },
    });

    const balanceAfter = await api.query.system.account(account.address);
    expect(balanceAfter.data.free.gt(balanceBefore.data.free)).to.be.true;
  });

  after(async () => {
    await aliceNode?.stop();
    await charlieNode?.stop();
    await webbRelayer?.stop();
  });
});
