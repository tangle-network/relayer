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
import { CircomUtxo, Keypair, parseTypedChainId, toFixedHex, Utxo } from '@webb-tools/sdk-core';

import { BigNumber, ethers } from 'ethers';
import temp from 'temp';
import { LocalChain, setupVanchorEvmTx } from '../../lib/localTestnet.js';
import {
  defaultWithdrawConfigValue,
  EnabledContracts,
  FeeInfo,
  ResourceMetricResponse,
  WebbRelayer,
} from '../../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { MintableToken } from '@webb-tools/tokens';
import { formatEther, parseEther } from 'ethers/lib/utils.js';

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
    parseTypedChainId;
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
    const wrappedToken1 = await localChain1.deployToken(
      'Wrapped Ethereum',
      'WETH'
    );
    const wrappedToken2 = await localChain2.deployToken(
      'Wrapped Ethereum',
      'WETH'
    );
    const unwrappedToken1 = await MintableToken.createToken(
      'Webb Token',
      'WEBB',
      govWallet1
    );
    const unwrappedToken2 = await MintableToken.createToken(
      'Webb Token',
      'WEBB',
      govWallet2
    );

    signatureVBridge = await localChain1.deploySignatureVBridge(
      localChain2,
      wrappedToken1,
      wrappedToken2,
      govWallet1,
      govWallet2,
      unwrappedToken1,
      unwrappedToken2
    );

    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      withdrawConfig: defaultWithdrawConfigValue,
      relayerWallet: relayerWallet1,
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge,
      withdrawConfig: defaultWithdrawConfigValue,
      relayerWallet: relayerWallet2,
    });

    // get the vanhor on localchain1
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(govWallet1);
    // get token
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      govWallet1
    );

    // aprove token spending for vanchor
    const tx = await token.approveSpending(
      vanchor1.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();

    // mint tokens on wallet
    await token.mintTokens(govWallet1.address, ethers.utils.parseEther('1000'));

    // do the same but on localchain2
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor2.setSigner(govWallet2);

    // Get token
    const tokenAddress2 = signatureVBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      govWallet2
    );

    // Approve token spending for vanchor
    const tx2 = await token2.approveSpending(
      vanchor2.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx2.wait();

    // Mint tokens on wallet
    await token2.mintTokens(
      govWallet2.address,
      ethers.utils.parseEther('1000')
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
      commonConfig: {
        features: { dataQuery: false, governanceRelay: false },
        port: relayerPort,
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
      verbosity: 3,
    });
    await webbRelayer.waitUntilReady();
  });

  // This test is meant to prove that utxo transfer flows are possible, and the receiver
  // can query on-chain data to construct and spend a utxo generated by the sender.
  it.only('should be able to transfer Utxo', async() => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(govWallet1);
    const vanchor2 = signatureVBridge.getVAnchor(localChain2.chainId);
    await vanchor2.setSigner(govWallet2);

    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;

    const token = await Tokens.MintableToken.tokenFromAddress(
        tokenAddress,
        govWallet1
      );
    
    // Step 1. Register recipient account(Bob)
    const bobKey = u8aToHex(ethers.utils.randomBytes(32));
    const bobWallet = new ethers.Wallet(bobKey, localChain1.provider());
    const bobKeypair = new Keypair();
    const tx = await vanchor1.contract.register({
        owner: govWallet1.address,
        keyData: bobKeypair.toString(),
      });

    let receipt = await tx.wait();
    // Step 2. Sender queries on chain data for keypair information of recipient
    // In this test, simply take the data from the previous transaction receipt.
    
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    //@ts-ignore
    const registeredKeydata: string = receipt.events[0].args.key;
    const bobPublicKeypair = Keypair.fromString(registeredKeydata);
    
    // Step 3. Generate a UTXO that is only spendable by recipient(Bob)
    const transferUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: ethers.utils.parseEther('10').toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain1.chainId.toString(),
      keypair: bobPublicKeypair,
    });
    
    // insert utxo into tree
    receipt = (await vanchor1.transact(
      [],
      [transferUtxo],
      0,
      0,
      '0',
      relayerWallet1.address,
      tokenAddress,
      {
       
      }
    )) as ethers.ContractReceipt;

    // Bob queries encrypted commitments on chain
    const encryptedCommitments: string[] = receipt.events?.filter((event) => event.event === 'NewCommitment')
    .sort((a, b) => a.args?.index - b.args?.index)
    .map((e) => e.args?.encryptedOutput) ?? [];

    // Attempt to decrypt the encrypted commitments with bob's keypair
    const utxos = await Promise.all(
      encryptedCommitments.map(async (enc, index) => {
        try {
          const decryptedUtxo = await CircomUtxo.decrypt(bobKeypair, enc);
          // In order to properly calculate the nullifier, an index is required.
          decryptedUtxo.setIndex(index);
          decryptedUtxo.setOriginChainId(localChain1.chainId.toString());
          const alreadySpent = await vanchor1.contract.isSpent(
            toFixedHex('0x' + decryptedUtxo.nullifier)
          );
          if (!alreadySpent) {
            return decryptedUtxo;
          } else {
            throw new Error('Passed Utxo detected as alreadySpent');
          }
        } catch (e) {
          return undefined;
        }
      })
    );
    
    const spendableUtxos = utxos.filter((utxo): utxo is Utxo => utxo !== undefined);

    // fetch the inserted leaves
    const leaves = vanchor1.tree.elements().map((leaf) => hexToU8a(leaf.toHexString()));
    // Bob uses the parsed utxos to issue a withdraw
    receipt = (await vanchor1.transact(
      spendableUtxos,
      [],
      0,
      0,
      bobWallet.address,
      relayerWallet1.address,
      tokenAddress,
      {
        [localChain1.chainId.toString()]: leaves,

      }
    )) as ethers.ContractReceipt;


    const bobBalanceAfter = await token.getBalance(bobWallet.address);

    console.log("Balance after ", bobBalanceAfter);


  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
