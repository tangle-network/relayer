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
// This our basic EVM Vanchor Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just relay transactions for us.

import { expect } from 'chai';
import { Tokens, VBridge } from '@webb-tools/protocol-solidity';
import { CircomUtxo } from '@webb-tools/sdk-core';
import { ethers } from 'ethers';
import temp from 'temp';
import { LocalChain } from '../../lib/localTestnet.js';
import {
  EnabledContracts,
  EncryptedOutputsCacheResponse,
  LeavesCacheResponse,
  WebbRelayer,
} from '../../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { hexToU8a, u8aToHex } from '@polkadot/util';

describe('Vanchor Transaction relayer', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureVBridge: VBridge.VBridge;
  let wallet1: ethers.Wallet;
  let wallet2: ethers.Wallet;
  let govWallet: ethers.Wallet;

  let webbRelayer: WebbRelayer;

  before(async () => {
    const PK1 = u8aToHex(ethers.utils.randomBytes(32));
    const PK2 = u8aToHex(ethers.utils.randomBytes(32));
    const GOV = u8aToHex(ethers.utils.randomBytes(32));
    // first we need to start local evm node.
    const localChain1Port = await getPort({
      port: portNumbers(3333, 4444),
    });

    const enabledContracts: EnabledContracts[] = [
      {
        contract: 'VAnchor',
      },
    ];
    localChain1 = await LocalChain.init({
      port: localChain1Port,
      chainId: localChain1Port,
      name: 'Hermes',
      populatedAccounts: [
        {
          secretKey: PK1,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
    });

    const localChain2Port = await getPort({
      port: portNumbers(3333, 4444),
    });
    localChain2 = await LocalChain.init({
      port: localChain2Port,
      chainId: localChain2Port,
      name: 'Athena',
      populatedAccounts: [
        {
          secretKey: PK2,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
    });

    wallet1 = new ethers.Wallet(PK1, localChain1.provider());
    wallet2 = new ethers.Wallet(PK2, localChain2.provider());
    govWallet = new ethers.Wallet(GOV, localChain2.provider());
    // Deploy the token.
    const localToken1 = await localChain1.deployToken('Webb Token', 'WEBB');
    const localToken2 = await localChain2.deployToken('Webb Token', 'WEBB');

    signatureVBridge = await localChain1.deploySignatureVBridge(
      localChain2,
      localToken1,
      localToken2,
      wallet1,
      wallet2,
      {
        [localChain1.chainId]: govWallet.address,
        [localChain2.chainId]: govWallet.address,
      }
    );

    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK2 },
    });

    // get the vanhor on localchain1
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(wallet1);
    // approve token spending
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    let tx = await token.approveSpending(
      vanchor1.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));

    // do the same but on localchain2
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor2.setSigner(wallet2);
    const tokenAddress2 = signatureVBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      wallet2
    );

    tx = await token2.approveSpending(
      vanchor2.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();
    await token2.mintTokens(wallet2.address, ethers.utils.parseEther('1000'));

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
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

  it('number of deposits made should be equal to number of leaves in cache', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);

    // set signers
    await vanchor1.setSigner(wallet1);
    await vanchor2.setSigner(wallet2);

    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token

    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));
    // check webbBalance
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    // Make 5 deposits
    for (let i = 0; i < 5; i++) {
      // Define inputs/outputs utxo for transact function
      const depositUtxo = await CircomUtxo.generateUtxo({
        curve: 'Bn254',
        backend: 'Circom',
        amount: (1e2).toString(),
        originChainId: localChain1.chainId.toString(),
        chainId: localChain1.chainId.toString(),
      });

      const leaves = vanchor1.tree
        .elements()
        .map((el) => hexToU8a(el.toHexString()));

      await vanchor1.transact([], [depositUtxo], 0, 0, '0', '0', tokenAddress, {
        [localChain1.chainId]: leaves,
      });
    }

    // now we wait for all deposits to be saved in LeafStorageCache
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '9',
      },
    });

    const expectedLeaves = vanchor1.tree
      .elements()
      .map((el) => Array.from(hexToU8a(el.toHexString())));
    // now we call relayer leaf API to check no of leaves stored in LeafStorageCache
    // are equal to no of deposits made. Each VAnchor deposit generates 2 leaf entries
    const chainId = localChain1.underlyingChainId.toString();
    const response = await webbRelayer.getLeavesEvm(
      chainId,
      vanchor1.contract.address
    );
    expect(response.status).equal(200);
    const leavesStore = response.json() as Promise<LeavesCacheResponse>;
    await leavesStore.then((resp) => {
      expect(resp.leaves.length).to.equal(10);
      expect(resp.leaves).to.deep.equal(expectedLeaves);
    });

    // Query a range of leaves.
    // We will try this with different scenarios
    // 1. Querying a range of leaves that are not present in the cache
    // 2. Querying a range of leaves that are present in the cache (1..5)
    // 2.1 Querying a range of leaves that are present in the cache (4..7).
    // 3. Querying a range of leaves that are partially present in the cache (1..12)
    // 4. Querying a range of leaves that is reverse (9..0)

    // 1. Querying a range of leaves that are not present in the cache
    const response1 = await webbRelayer.getLeavesEvm(
      chainId,
      vanchor1.contract.address,
      { start: 20, end: 30 }
    );
    expect(response1.status).equal(200);
    const leavesStore1 = response1.json() as Promise<LeavesCacheResponse>;
    await leavesStore1.then((resp) => {
      expect(resp.leaves.length).to.equal(0);
    });

    // 2. Querying a range of leaves that are present in the cache (1..5)
    // We will query leaves from 1 to 5
    const response2 = await webbRelayer.getLeavesEvm(
      chainId,
      vanchor1.contract.address,
      { start: 1, end: 5 }
    );
    expect(response2.status).equal(200);
    const leavesStore2 = response2.json() as Promise<LeavesCacheResponse>;
    await leavesStore2.then((resp) => {
      expect(resp.leaves.length).to.equal(4);
      expect(resp.leaves).to.deep.equal(expectedLeaves.slice(1, 5));
    });

    // 2.1 Querying a range of leaves that are present in the cache (4..7).
    // We will query leaves from 4 to 7
    const response21 = await webbRelayer.getLeavesEvm(
      chainId,
      vanchor1.contract.address,
      { start: 4, end: 7 }
    );
    expect(response21.status).equal(200);
    const leavesStore21 = response21.json() as Promise<LeavesCacheResponse>;
    await leavesStore21.then((resp) => {
      expect(resp.leaves.length).to.equal(3);
      expect(resp.leaves).to.deep.equal(expectedLeaves.slice(4, 7));
    });

    // 3. Querying a range of leaves that are partially present in the cache (1..12)
    // We will query leaves from 1 to 12
    const response3 = await webbRelayer.getLeavesEvm(
      chainId,
      vanchor1.contract.address,
      { start: 1, end: 12 }
    );
    expect(response3.status).equal(200);
    const leavesStore3 = response3.json() as Promise<LeavesCacheResponse>;
    await leavesStore3.then((resp) => {
      expect(resp.leaves.length).to.equal(9);
      expect(resp.leaves).to.deep.equal(expectedLeaves.slice(1, 12));
    });

    // 4. Querying a range of leaves that is reverse (9..0)
    // We will query leaves from 9 to 0
    // This should return only one item, the last leaf
    // It assumes that we want to start from index 9 and ends at index 0
    // but we do not support reverse indices, hence we ignore the end index
    // as long as it is less than the start index
    const response4 = await webbRelayer.getLeavesEvm(
      chainId,
      vanchor1.contract.address,
      { start: 9, end: 0 }
    );
    expect(response4.status).equal(200);
    const leavesStore4 = response4.json() as Promise<LeavesCacheResponse>;
    await leavesStore4.then((resp) => {
      expect(resp.leaves.length).to.equal(1);
      expect(resp.leaves).to.deep.equal(expectedLeaves.slice(9));
    });
  });

  it('number of deposits made should be equal to number of encrypted outputs in cache', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);

    // set signers
    await vanchor1.setSigner(wallet1);
    await vanchor2.setSigner(wallet2);

    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token

    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(
      wallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );
    // check webbBalance
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    // Make 5 deposits
    for (let i = 0; i < 5; i++) {
      // Define inputs/outputs utxo for transact function
      const depositUtxo = await CircomUtxo.generateUtxo({
        curve: 'Bn254',
        backend: 'Circom',
        amount: (1e2).toString(),
        originChainId: localChain1.chainId.toString(),
        chainId: localChain1.chainId.toString(),
      });

      const leaves = vanchor1.tree
        .elements()
        .map((el) => hexToU8a(el.toHexString()));

      await vanchor1.transact([], [depositUtxo], 0, 0, '0', '0', tokenAddress, {
        [localChain1.chainId]: leaves,
      });
    }

    await webbRelayer.waitForEvent({
      kind: 'encrypted_outputs_store',
      event: {
        encrypted_output_index: '9',
      },
    });

    // now we call relayer leaf API to check no of leaves stored in LeafStorageCache
    // are equal to no of deposits made. Each VAnchor deposit generates 2 leaf entries
    const chainId = localChain1.underlyingChainId.toString();

    const response = await webbRelayer.getEncryptedOutputsEvm(
      chainId,
      vanchor1.contract.address
    );
    expect(response.status).equal(200);

    const store = response.json() as Promise<EncryptedOutputsCacheResponse>;
    await store.then((resp) => {
      expect(resp.encryptedOutputs.length).to.equal(18);
    });

    const response1 = await webbRelayer.getEncryptedOutputsEvm(
      chainId,
      vanchor1.contract.address,
      { start: 20, end: 30 }
    );
    expect(response1.status).equal(200);
    const store1 = response1.json() as Promise<EncryptedOutputsCacheResponse>;
    await store1.then((resp) => {
      expect(resp.encryptedOutputs.length).to.equal(0);
    });

    const response2 = await webbRelayer.getEncryptedOutputsEvm(
      chainId,
      vanchor1.contract.address,
      { start: 1, end: 5 }
    );
    expect(response2.status).equal(200);
    const store2 = response2.json() as Promise<EncryptedOutputsCacheResponse>;
    await store2.then((resp) => {
      expect(resp.encryptedOutputs.length).to.equal(4);
    });
  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
