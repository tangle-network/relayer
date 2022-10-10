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
// const assert = require('assert');
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
    const localToken1 = await localChain1.deployToken(
      'Webb Token',
      'WEBB',
      wallet1
    );
    const localToken2 = await localChain2.deployToken(
      'Webb Token',
      'WEBB',
      wallet2
    );

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
    let tx = await token.approveSpending(vanchor1.contract.address);
    await tx.wait();
    await token.mintTokens(
      wallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );

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

    tx = await token2.approveSpending(vanchor2.contract.address);
    await tx.wait();
    await token2.mintTokens(
      wallet2.address,
      ethers.utils.parseEther('100000000000000000000000')
    );

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      commonConfig: {
        port: relayerPort
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: false,
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

      await vanchor1.transact(
        [],
        [depositUtxo],
        {
          [localChain1.chainId]: leaves,
        },
        '0',
        '0',
        '0',
        '0'
      );
    }

    // now we wait for all deposits to be saved in LeafStorageCache
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '9',
      },
    });

    // now we call relayer leaf API to check no of leaves stored in LeafStorageCache
    // are equal to no of deposits made. Each VAnchor deposit generates 2 leaf entries
    const chainId = localChain1.underlyingChainId.toString(16);
    const response = await webbRelayer.getLeavesEvm(
      chainId,
      vanchor1.contract.address
    );
    expect(response.status).equal(200);
    const leavesStore = response.json() as Promise<LeavesCacheResponse>;
    const result = leavesStore.then((resp) => {
      expect(resp.leaves.length).to.equal(10);
      return true;
    });
    expect(result).to.eq(true);
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

      await vanchor1.transact(
        [],
        [depositUtxo],
        {
          [localChain1.chainId]: leaves,
        },
        '0',
        '0',
        '0',
        '0'
      );
    }

    await webbRelayer.waitForEvent({
      kind: 'encrypted_outputs_store',
      event: {
        encrypted_output_index: '9',
      },
    });

    // now we call relayer leaf API to check no of leaves stored in LeafStorageCache
    // are equal to no of deposits made. Each VAnchor deposit generates 2 leaf entries
    const chainId = localChain1.underlyingChainId.toString(16);

    const response = await webbRelayer.getEncryptedOutputsEvm(
      chainId,
      vanchor1.contract.address
    );
    expect(response.status).equal(200);

    const store = response.json() as Promise<EncryptedOutputsCacheResponse>;
    const result = store.then((resp) => {
      expect(resp.encrypted_outputs.length).to.equal(10);
      return true;
    });
    expect(result).to.eq(true);
  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
