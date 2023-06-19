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

import { assert } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import isCi from 'is-ci';
import { WebbRelayer, Pallet } from '../../lib/webbRelayer.js';
import { BigNumber, ethers } from 'ethers';
import { ApiPromise } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import {
  Utxo,
  calculateTypedChainId,
  ChainType,
  Keypair,
  CircomUtxo,
  randomBN,
  toFixedHex,
  LeafIdentifier,
  FIELD_SIZE,
  buildVariableWitnessCalculator,
  generateVariableWitnessInput,
  MerkleProof,
  MerkleTree,
} from '@webb-tools/sdk-core';
import { currencyToUnitI128, UsageMode } from '@webb-tools/test-utils';
import { createAccount, defaultEventsWatcherValue } from '../../lib/utils.js';
import { vanchorDeposit, vanchorWithdraw } from '../../lib/substrateVAnchor.js';
import { LocalTangle } from '../../lib/localTangle.js';
import { fetchComponentsFromFilePaths } from '@webb-tools/utils';
import { HexString } from '@polkadot/util/types';

describe.only('Substrate VAnchor Private Transaction Relayer Tests Using Circom', function() {
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalTangle;
  let bobNode: LocalTangle;
  let charlieNode: LocalTangle;
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

    aliceNode = await LocalTangle.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });

    bobNode = await LocalTangle.start({
      name: 'substrate-bob',
      authority: 'bob',
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
      showLogs: false,
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

    const account = createAccount('//Dave');
    const amount = 1000000000;

    // 2. Deposit amount on substrate chain.
    const data = await vanchorDeposit(
      account,
      treeId,
      typedSourceChainId.toString(), // source chain Id
      typedSourceChainId.toString(), // target chain Id
      amount, // public amount
      aliceNode
    );

    // 3. Now we withdraw it.
    await vanchorWithdraw(
      treeId,
      typedSourceChainId.toString(),
      typedSourceChainId.toString(),
      data.depositUtxos,
      aliceNode
    );

    // Withdraw Flow
    /*
    // 3. Now we withdraw it on bob's account using private transaction.
    const account = createAccount('//Bob');
    // Bob's balance after withdrawal
    const bobBalanceBefore = await api.query.system.account(account.address);

    const dummyVanchorData = await vanchorWithdraw(
      typedSourceChainId.toString(),
      typedSourceChainId.toString(),
      account.address,
      data.depositUtxos,
      treeId,
      BigInt(0),
      // TODO: need to convert this once there is exchange rate between native
      //       token and wrapped token
      BigInt(0),
      api
    );
    const token = new DataView(dummyVanchorData.extData.token.buffer, 0);

    const info = await api.tx.vAnchorBn254
      .transact(treeId, dummyVanchorData.proofData, dummyVanchorData.extData)
      .paymentInfo(account);
    const feeInfoResponse2 = await webbRelayer.getSubstrateFeeInfo(
      substrateChainId,
      info.partialFee.toBn()
    );
    expect(feeInfoResponse2.status).equal(200);
    const feeInfo2 =
      await (feeInfoResponse2.json() as Promise<SubstrateFeeInfo>);
    const estimatedFee = BigInt(feeInfo2.estimatedFee);

    const refund = BigInt(0);
    const feeTotal = estimatedFee + refund;
    const vanchorData = await vanchorWithdraw(
      typedSourceChainId.toString(),
      typedSourceChainId.toString(),
      account.address,
      data.depositUtxos,
      treeId,
      feeTotal,
      // TODO: need to convert this once there is exchange rate between native
      //       token and wrapped token
      refund,
      api
    );
    const substrateExtData: SubstrateVAnchorExtData = {
      recipient: vanchorData.extData.recipient,
      relayer: vanchorData.extData.relayer,
      extAmount: BigNumber.from(vanchorData.extData.extAmount)
        .toHexString()
        .replace('0x', ''),
      fee: BigNumber.from(feeTotal.toString()).toHexString(),
      encryptedOutput1: Array.from(
        hexToU8a(vanchorData.extData.encryptedOutput1)
      ),
      encryptedOutput2: Array.from(
        hexToU8a(vanchorData.extData.encryptedOutput2)
      ),
      refund: BigNumber.from(refund.toString()).toHexString(),
      token: token.getUint32(0, true),
    };

    const substrateProofData: SubstrateVAnchorProofData = {
      proof: Array.from(hexToU8a(vanchorData.proofData.proof)),
      extDataHash: Array.from(vanchorData.proofData.extDataHash),
      extensionRoots: vanchorData.proofData.roots.map((root) =>
        Array.from(root)
      ),
      publicAmount: Array.from(vanchorData.proofData.publicAmount),
      roots: vanchorData.proofData.roots.map((root) => Array.from(root)),
      outputCommitments: vanchorData.proofData.outputCommitments.map((com) =>
        Array.from(com)
      ),
      inputNullifiers: vanchorData.proofData.inputNullifiers.map((com) =>
        Array.from(hexToU8a(com))
      ),
    };

    // now we withdraw using private transaction
    await webbRelayer.substrateVAnchorWithdraw(
      substrateChainId,
      treeId,
      substrateExtData,
      substrateProofData
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

    // Bob's balance after withdrawal.
    const BobBalanceAfter = await api.query.system.account(account.address);
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    //@ts-ignore
    console.log('balance after : ', BobBalanceAfter.data.free);
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    //@ts-ignore
    assert(BobBalanceAfter.data.free > bobBalanceBefore.data.free);
    */
  });

  after(async () => {
    await aliceNode?.stop();
    await bobNode?.stop();
    await charlieNode?.stop();
    await webbRelayer?.stop();
  });
});
