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
import { Anchors, Tokens, VBridge } from '@webb-tools/protocol-solidity';
import { CircomUtxo, Keypair, Utxo } from '@webb-tools/sdk-core';
import {
  IVariableAnchorExtData,
  IVariableAnchorPublicInputs,
} from '@webb-tools/interfaces';
import { ethers, Wallet } from 'ethers';
import retry from 'async-retry';
import temp from 'temp';
import { LocalChain, setupVanchorEvmTx } from '../../lib/localTestnet.js';
import {
  defaultWithdrawConfigValue,
  EnabledContracts,
  WebbRelayer,
} from '../../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { u8aToHex, hexToU8a } from '@polkadot/util';

// const assert = require('assert');
describe('Vanchor Private Tx relaying with mocked governor', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureVBridge: VBridge.VBridge;
  let govWallet1: ethers.Wallet;
  let govWallet2: ethers.Wallet;
  let relayerWallet1: ethers.Wallet;
  let relayerWallet2: ethers.Wallet;

  let webbRelayer: WebbRelayer;

  before(async () => {
    const govPk = u8aToHex(ethers.utils.randomBytes(32));
    const relayerPk = u8aToHex(ethers.utils.randomBytes(32));

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
          secretKey: govPk,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
        {
          secretKey: relayerPk,
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
          secretKey: govPk,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
        {
          secretKey: relayerPk,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
    });

    govWallet1 = new ethers.Wallet(govPk, localChain1.provider());
    govWallet2 = new ethers.Wallet(govPk, localChain2.provider());
    relayerWallet1 = new ethers.Wallet(relayerPk, localChain1.provider());
    relayerWallet2 = new ethers.Wallet(relayerPk, localChain2.provider());
    // Deploy the token.
    const localToken1 = await localChain1.deployToken(
      'Webb Token',
      'WEBB',
      govWallet1
    );
    const localToken2 = await localChain2.deployToken(
      'Webb Token',
      'WEBB',
      govWallet2
    );

    console.log('deploying vbridge...')

    signatureVBridge = await localChain1.deploySignatureVBridge(
      localChain2,
      localToken1,
      localToken2,
      govWallet1,
      govWallet2
    );

    console.log('deployed the vbridge');

    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      features: { dataQuery: false, governanceRelay: false },
      withdrawConfig: defaultWithdrawConfigValue,
      relayerWallet: relayerWallet1,
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge,
      features: { dataQuery: false, governanceRelay: false },
      withdrawConfig: defaultWithdrawConfigValue,
      relayerWallet: relayerWallet2,
    });

    // get the vanhor on localchain1
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
    await vanchor1.setSigner(govWallet1);
    // approve token spending
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );
    let tx = await token.approveSpending(vanchor1.contract.address);
    await tx.wait();
    await token.mintTokens(
      govWallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );

    // do the same but on localchain2
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId)!;
    await vanchor2.setSigner(govWallet2);
    const tokenAddress2 = signatureVBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      govWallet2
    );

    tx = await token2.approveSpending(vanchor2.contract.address);
    await tx.wait();
    await token2.mintTokens(
      govWallet2.address,
      ethers.utils.parseEther('100000000000000000000000')
    );

    // Set governor
    const governorAddress = govWallet1.address;
    const currentGovernor = await signatureVBridge
      .getVBridgeSide(localChain1.chainId)
      .contract.governor();
    expect(currentGovernor).to.eq(governorAddress);

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });

    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
      showLogs: false,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should relay private transaction', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
    await vanchor1.setSigner(govWallet1);

    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId)!;
    await vanchor2.setSigner(govWallet2);

    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(
      govWallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );
    // check webbBalance
    const webbBalance = await token.getBalance(govWallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const randomKeypair = new Keypair();

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // SignatureVBridge will transact and update the anchors
    await signatureVBridge.transact([], [depositUtxo], 0, '0', '0', govWallet1);

    // now we wait for the relayer to see the transaction
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    });

    let output = await setupVanchorEvmTx(
      depositUtxo,
      localChain1,
      localChain2,
      randomKeypair,
      vanchor1,
      vanchor2,
      relayerWallet2
    );

    await webbRelayer.vanchorWithdraw(
      localChain2.underlyingChainId,
      vanchor2.getAddress(),
      output.publicInputs,
      output.extData
    );

    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    });
  });

  it('Should fail to withdraw with invalid root', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
    await vanchor1.setSigner(govWallet1);

    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId)!;
    await vanchor2.setSigner(govWallet2);

    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token

    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(
      govWallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );
    // check webbBalance
    const webbBalance = await token.getBalance(govWallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const randomKeypair = new Keypair();

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // SignatureVBridge will transact and update the anchors
    await signatureVBridge.transact([], [depositUtxo], 0, '0', '0', govWallet1);

    // now we wait for the relayer to see the transaction
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    });

    let output = await setupVanchorEvmTx(
      depositUtxo,
      localChain1,
      localChain2,
      randomKeypair,
      vanchor1,
      vanchor2,
      relayerWallet2
    );

    const rootBytes = hexToU8a(output.publicInputs.roots);
    // flip a bit in the proof, so it is invalid
    rootBytes[0] = 0x42;

    const invalidRootBytes = u8aToHex(rootBytes);
    expect(output.publicInputs.roots).to.not.eq(invalidRootBytes);
    output.publicInputs.roots = invalidRootBytes;
    try {
      await webbRelayer.vanchorWithdraw(
        localChain2.underlyingChainId,
        vanchor2.getAddress(),
        output.publicInputs,
        output.extData
      );
    } catch (e) {
      // should fail since private transaction since invalid merkle root is provided.
      expect(JSON.stringify(e)).to.contain('Cannot find your merkle root');
    }
  });

  it('Should fail to withdraw with invalid proof', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
    await vanchor1.setSigner(govWallet1);

    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId)!;
    await vanchor2.setSigner(govWallet2);

    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token

    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(
      govWallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );
    // check webbBalance
    const webbBalance = await token.getBalance(govWallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const randomKeypair = new Keypair();

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // SignatureVBridge will transact and update the anchors
    await signatureVBridge.transact([], [depositUtxo], 0, '0', '0', govWallet1);

    // now we wait for the relayer to see the transaction
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    });

    let output = await setupVanchorEvmTx(
      depositUtxo,
      localChain1,
      localChain2,
      randomKeypair,
      vanchor1,
      vanchor2,
      relayerWallet2
    );

    const proofBytes = hexToU8a(output.publicInputs.proof);
    // flip a bit in the proof, so it is invalid
    proofBytes[0] = 0x42;
    const invalidProofBytes = u8aToHex(proofBytes);
    expect(output.publicInputs.roots).to.not.eq(invalidProofBytes);
    output.publicInputs.proof = invalidProofBytes;
    try {
      await webbRelayer.vanchorWithdraw(
        localChain2.underlyingChainId,
        vanchor2.getAddress(),
        output.publicInputs,
        output.extData
      );
    } catch (e) {
      // should fail since private transaction since invalid proof is provided
      expect(JSON.stringify(e)).to.contain(
        'Exception while processing transaction'
      );
    }
  });

  it('Should fail to withdraw with invalid nullifier hash', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
    await vanchor1.setSigner(govWallet1);

    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId)!;
    await vanchor2.setSigner(govWallet2);

    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    // get token

    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );
    // mint tokens to the account everytime.
    await token.mintTokens(
      govWallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );
    // check webbBalance
    const webbBalance = await token.getBalance(govWallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const randomKeypair = new Keypair();

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // SignatureVBridge will transact and update the anchors
    await signatureVBridge.transact([], [depositUtxo], 0, '0', '0', govWallet1);

    // now we wait for the relayer to see the transaction
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    });

    let output = await setupVanchorEvmTx(
      depositUtxo,
      localChain1,
      localChain2,
      randomKeypair,
      vanchor1,
      vanchor2,
      relayerWallet2
    );

    const nullifierHash = hexToU8a(output.publicInputs.inputNullifiers[0]);
    // flip a bit in the proof, so it is invalid
    nullifierHash[0] = 0x42;

    const invalidnullifierHash = u8aToHex(nullifierHash);
    expect(output.publicInputs.inputNullifiers[0]).to.not.eq(
      invalidnullifierHash
    );
    output.publicInputs.proof = invalidnullifierHash;
    try {
      await webbRelayer.vanchorWithdraw(
        localChain2.underlyingChainId,
        vanchor2.getAddress(),
        output.publicInputs,
        output.extData
      );
    } catch (e) {
      // should fail since private transaction since invalid proof is provided
      expect(JSON.stringify(e)).to.contain(
        'Exception while processing transaction'
      );
    }
  });

  it('should fail to query leaves data api', async () => {
    this.retries(0);
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
    // It should fail since data querying is not configured for relayer
    const chainId = localChain1.underlyingChainId.toString(16);
    const response = await webbRelayer.getLeavesEvm(
      chainId,
      vanchor1.contract.address
    );
    //forbidden access
    expect(response.status).equal(403);
  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
