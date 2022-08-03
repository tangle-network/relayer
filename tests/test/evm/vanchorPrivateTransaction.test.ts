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
import { CircomUtxo, Keypair, randomBN, Utxo } from '@webb-tools/sdk-core';
import { BigNumber, ethers } from 'ethers';
import temp from 'temp';
import { LocalChain } from '../../lib/localTestnet.js';
import {
  calculateRelayerFees,
  defaultWithdrawConfigValue,
  EnabledContracts,
  LeavesCacheResponse,
  WebbRelayer,
} from '../../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { u8aToHex , hexToU8a} from '@polkadot/util';
// const assert = require('assert');
describe.only('Vanchor Private Transaction relayer', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureVBridge: VBridge.VBridge;
  let wallet1: ethers.Wallet;
  let wallet2: ethers.Wallet;

  let webbRelayer: WebbRelayer;

  before(async () => {
    const PK1 = u8aToHex(ethers.utils.randomBytes(32));
    const PK2 = u8aToHex(ethers.utils.randomBytes(32));
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
    localChain1 = new LocalChain({
      port: localChain1Port,
      chainId: 31337,
      name: 'Hermes',
      populatedAccounts: [
        {
          secretKey: PK1,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        },
        {
          secretKey: relayerPk,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        }
      ],
      enabledContracts: enabledContracts,
    });

    const localChain2Port = await getPort({
      port: portNumbers(3333, 4444),
    });
    localChain2 = new LocalChain({
      port: localChain2Port,
      chainId: 5002,
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
    let relayerWallet = new ethers.Wallet(relayerPk, localChain1.provider());
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
      wallet2
    );

    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
      features: { dataQuery: false, governanceRelay: false },
      withdrawConfig: defaultWithdrawConfigValue,
      relayerWallet: relayerWallet
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK2 },
      features: { dataQuery: false, governanceRelay: false },
      relayerWallet: relayerWallet
    });

    // get the vanhor on localchain1
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
    await vanchor1.setSigner(wallet1);
    // approve token spending
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    await token.approveSpending(vanchor1.contract.address);
    await token.mintTokens(
      wallet1.address,
      ethers.utils.parseEther('100000000000000000000000')
    );

    // do the same but on localchain2
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId)!;
    await vanchor2.setSigner(wallet2);
    const tokenAddress2 = signatureVBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      wallet2
    );

    await token2.approveSpending(vanchor2.contract.address);
    await token2.mintTokens(
      wallet2.address,
      ethers.utils.parseEther('100000000000000000000000')
    );

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should relay private transaction', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId)!;
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId)!;

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
    
    // get allowance 
    await token.getAllowance(vanchor1.contract.address, '10000000000000000000000');
    // approve the anchor to spend the minted funds
  
    const tx = await token.approveSpending(tokenAddress);
    await tx.wait();
    // check webbBalance
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const recipient = new ethers.Wallet(
      ethers.utils.randomBytes(32),
      localChain1.provider()
    );
    const relayerInfo = await webbRelayer.info();
    const localChain1Info = relayerInfo.evm[localChain1.underlyingChainId];

    const relayerFeePercentage =
      localChain1Info?.contracts.find(
        (c) => c.address === vanchor1.contract.address
      )?.withdrawConfig?.withdrawFeePercentage ?? 0;

   
    let inputs:Utxo[] = [];
    const randomKeypair = new Keypair();

    while (inputs.length !== 2 && inputs.length < 16) {
      inputs.push(await CircomUtxo.generateUtxo({
        curve: 'Bn254',
        backend: 'Circom',
        chainId: localChain1.chainId.toString(),
        originChainId: localChain1.chainId.toString(),
        amount: '0',
        blinding: hexToU8a(randomBN(31).toHexString()),
        keypair: randomKeypair
      }));
    }

    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: '1000',
      originChainId: localChain1.chainId.toString(),
      chainId: localChain1.chainId.toString(),
    });

    const dummyUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: '0',
      chainId: localChain1.chainId.toString(),
      keypair: randomKeypair,
    });

    const outputs = [depositUtxo, dummyUtxo];
    const leaves = vanchor1.tree
      .elements()
      .map((el) => hexToU8a(el.toHexString()));
    
    const leavesMap = {
      [localChain1.chainId]: leaves,
    };
    
    let extAmount = BigNumber.from(relayerFeePercentage)
      .add(outputs.reduce((sum, x) => sum.add(x.amount), BigNumber.from(0)))
      .sub(inputs.reduce((sum, x) => sum.add(x.amount), BigNumber.from(0)))

    
    const { extData,publicInputs } = await vanchor1.setupTransaction(
      inputs,
      [depositUtxo, dummyUtxo],
      extAmount,
      relayerFeePercentage,
      recipient.address,
      wallet1.address,
      leavesMap,
    );
    console.log("ExtData : ", extData);
    console.log("publicInputs : ", publicInputs);
    // now send the withdrawal request.
    const txHash = await webbRelayer.vanchorWithdraw(
      localChain1.underlyingChainId,
      vanchor1.getAddress(),
      publicInputs,
      extData
    );
    expect(txHash).to.be.string;

  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
