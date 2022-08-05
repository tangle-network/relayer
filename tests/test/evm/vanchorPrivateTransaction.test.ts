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
import { CircomUtxo, Keypair } from '@webb-tools/sdk-core';
import { ethers } from 'ethers';
import retry from 'async-retry';
import temp from 'temp';
import { LocalChain } from '../../lib/localTestnet.js';
import {
  defaultWithdrawConfigValue,
  EnabledContracts,
  LeavesCacheResponse,
  WebbRelayer,
} from '../../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { u8aToHex } from '@polkadot/util';
import { hexToU8a } from '@webb-tools/utils';
import { sleep } from '../../lib/sleep.js';

// const assert = require('assert');
describe.only('Vanchor Private Tx relaying with mocked governor', function () {
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
    localChain1 = new LocalChain({
      port: localChain1Port,
      chainId: 31337,
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
        }
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

    signatureVBridge = await localChain1.deploySignatureVBridge(
      localChain2,
      localToken1,
      localToken2,
      govWallet1,
      govWallet2
    );

    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      features: { dataQuery: false, governanceRelay: false },
      withdrawConfig: defaultWithdrawConfigValue,
      relayerWallet: relayerWallet1
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge,
      features: { dataQuery: false, governanceRelay: false },
      withdrawConfig: defaultWithdrawConfigValue,
      relayerWallet: relayerWallet2
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
    await token.approveSpending(vanchor1.contract.address);
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

    await token2.approveSpending(vanchor2.contract.address);
    await token2.mintTokens(
      govWallet2.address,
      ethers.utils.parseEther('100000000000000000000000')
    );

    // Set governor
    const governorAddress = govWallet1.address;
    const currentGovernor = await signatureVBridge.getVBridgeSide(localChain1.chainId).contract.governor();
    expect(currentGovernor).to.eq(governorAddress);
   
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

    // set signers
    await vanchor1.setSigner(govWallet1);

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
      keypair: randomKeypair
    });
    let leaves1 = vanchor1.tree
      .elements()
      .map((el) => hexToU8a(el.toHexString()));

    // SignatureVBridge will transact and update the anchors
    await signatureVBridge.transact([], [depositUtxo], 0, '0', '0', govWallet1);
    
    // now we wait for the relayer to see the transaction
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    })
    
    let extAmount = ethers.BigNumber.from(0)
      .sub(depositUtxo.amount)

    const dummyOutput1 = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: '0',
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    const dummyOutput2 = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom', 
      amount: '0',
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    const dummyInput = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: '0',
      chainId: localChain2.chainId.toString(),
      originChainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    })

    const recipient = '0x0000000001000000000100000000010000000001';

    // Populate the leavesMap for generating the zkp against the source chain
    // 
    leaves1 = vanchor1.tree
      .elements()
      .map((el) => hexToU8a(el.toHexString()));
    
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId)!;
    await vanchor2.setSigner(govWallet2);

    const leaves2 = vanchor2.tree
     .elements()
     .map((el) => hexToU8a(el.toHexString()));

    const depositUtxoIndex = vanchor1.tree.getIndexByElement(u8aToHex(depositUtxo.commitment));

    const regeneratedUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: depositUtxo.amount,
      chainId: depositUtxo.chainId,
      originChainId: depositUtxo.originChainId,
      blinding: hexToU8a(depositUtxo.blinding),
      privateKey: hexToU8a(depositUtxo.secret_key),
      keypair: randomKeypair,
      index: depositUtxoIndex.toString(),
    })

    const leavesMap = {
      [localChain1.chainId]: leaves1,
      [localChain2.chainId]: leaves2,
    };

    const { extData,publicInputs } = await vanchor2.setupTransaction(
      [regeneratedUtxo, dummyInput],
      [dummyOutput1, dummyOutput2],
      extAmount,
      0,
      recipient,
      relayerWallet2.address,
      leavesMap,
    );

    await webbRelayer.vanchorWithdraw(5002, vanchor2.getAddress(), publicInputs, extData);

    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: '1',
      },
    })
  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
