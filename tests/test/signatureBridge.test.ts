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
// A simple test for the Signature Bridge with the Relayer and the DKG.

import Chai, { expect } from 'chai';
import ChaiAsPromised from 'chai-as-promised';
import { Tokens, VBridge } from '@webb-tools/protocol-solidity';
import { u8aToHex } from '@polkadot/util';
import { ethers } from 'ethers';
import temp from 'temp';
import retry from 'async-retry';
import { LocalChain } from '../lib/localTestnet.js';
import { Pallet, WebbRelayer, EnabledContracts } from '../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { LocalDkg } from '../lib/localDkg.js';
import isCi from 'is-ci';
import path from 'path';
import { ethAddressFromUncompressedPublicKey } from '../lib/ethHelperFunctions.js';
import {
  defaultEventsWatcherValue,
  UsageMode,
} from '../lib/substrateNodeBase.js';
import { CircomUtxo, Keypair } from '@webb-tools/sdk-core';

// to support chai-as-promised
Chai.use(ChaiAsPromised);

describe.skip('Signature Bridge <> DKG Proposal Signing Backend', function () {
  this.timeout(5 * 60 * 1000);
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureBridge: VBridge.VBridge;
  let wallet1: ethers.Wallet;
  let wallet2: ethers.Wallet;

  // dkg nodes
  let aliceNode: LocalDkg;
  let bobNode: LocalDkg;
  let charlieNode: LocalDkg;

  let webbRelayer: WebbRelayer;

  before(async function () {
    // skip this test if we are locally running, only run on the CI
    if (!isCi) this.skip();
    const PK1 = u8aToHex(ethers.utils.randomBytes(32));
    const PK2 = u8aToHex(ethers.utils.randomBytes(32));
    const usageMode: UsageMode = isCi
      ? { mode: 'host', nodePath: 'dkg-standalone-node' }
      : {
          mode: 'host',
          nodePath: path.resolve(
            '../../dkg-substrate/target/release/dkg-standalone-node'
          ),
        };
    const enabledPallets: Pallet[] = [
      {
        pallet: 'DKGProposalHandler',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];
    aliceNode = await LocalDkg.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enabledPallets,
    });

    bobNode = await LocalDkg.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode,
      ports: 'auto',
      enabledPallets,
    });

    charlieNode = await LocalDkg.start({
      name: 'substrate-charlie',
      authority: 'charlie',
      usageMode,
      ports: 'auto',
      enableLogging: false,
      enabledPallets,
    });
    // get chainId
    let chainId = await charlieNode.getChainId();
    await charlieNode.writeConfig(`${tmpDirPath}/${charlieNode.name}.json`, {
      suri: '//Charlie',
      chainId: chainId,
    });

    // we need to wait until the public key is on chain.
    await charlieNode.waitForEvent({
      section: 'dkg',
      method: 'PublicKeySubmitted',
    });

    // next we need to start local evm node.
    const localChain1Port = await getPort({
      port: portNumbers(3333, 4444),
    });

    const enabledContracts: EnabledContracts[] = [
      {
        contract: 'Anchor',
      },
    ];

    localChain1 = await LocalChain.init({
      port: localChain1Port,
      chainId: localChain1Port,
      name: 'Hermes',
      populatedAccounts: [
        {
          secretKey: PK1,
          balance: ethers.utils.parseEther('1000').toHexString(),
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
          balance: ethers.utils.parseEther('1000').toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
    });

    wallet1 = new ethers.Wallet(PK1, localChain1.provider());
    wallet2 = new ethers.Wallet(PK2, localChain2.provider());
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

    signatureBridge = await localChain1.deploySignatureVBridge(
      localChain2,
      localToken1,
      localToken2,
      wallet1,
      wallet2
    );
    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge: signatureBridge,
      proposalSigningBackend: { type: 'DKGNode', node: charlieNode.name },
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge: signatureBridge,
      proposalSigningBackend: { type: 'DKGNode', node: charlieNode.name },
    });
    // fetch the dkg public key.
    const dkgPublicKey = await charlieNode.fetchDkgPublicKey();
    expect(dkgPublicKey).to.not.be.null;
    const governorAddress = ethAddressFromUncompressedPublicKey(dkgPublicKey!);
    // verify the governor address is a valid ethereum address.
    expect(ethers.utils.isAddress(governorAddress)).to.be.true;
    // transfer ownership to the DKG.
    const sides = signatureBridge.vBridgeSides.values();
    for (const signatureSide of sides) {
      // now we transferOwnership, forcefully.
      const tx = await signatureSide.transferOwnership(governorAddress, 1);
      await retry(
        async () => {
          await tx.wait();
        },
        {
          retries: 5,
          minTimeout: 1000,
          onRetry: (error) => {
            console.error('transferOwnership retry', error.name, error.message);
          },
        }
      );
      // check that the new governor is the same as the one we just set.
      const currentGovernor = await signatureSide.contract.governor();
      expect(currentGovernor).to.eq(governorAddress);
    }
    // get the anhor on localchain1
    const anchor = signatureBridge.getVAnchor(
      localChain1.chainId,
    )!;
    await anchor.setSigner(wallet1);
    // approve token spending
    const tokenAddress = signatureBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    let tx = await token.approveSpending(anchor.contract.address);
    await tx.wait();
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));

    // do the same but on localchain2
    const anchor2 = signatureBridge.getVAnchor(
      localChain2.chainId,
    )!;
    await anchor2.setSigner(wallet2);
    const tokenAddress2 = signatureBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      wallet2
    );

    tx = await token2.approveSpending(anchor2.contract.address);
    await tx.wait();
    await token2.mintTokens(wallet2.address, ethers.utils.parseEther('1000'));

    const api = await charlieNode.api();
    const resourceId1 = await anchor.createResourceId();
    const resourceId2 = await anchor2.createResourceId();

    const call = (resourceId: string) =>
      api.tx.dkgProposals.setResource(resourceId, '0x00');
    // register the resource on DKG node.
    for (const rid of [resourceId1, resourceId2]) {
      await charlieNode.sudoExecuteTransaction(call(rid));
    }
    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
      showLogs: false,
      verbosity: 3,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should handle AnchorUpdateProposal when a deposit happens using DKG proposal backend', async () => {
    // we will use chain1 as an example here.
    const anchor1 = signatureBridge.getVAnchor(
      localChain1.chainId,
    );
    const anchor2 = signatureBridge.getVAnchor(
      localChain2.chainId,
    );
    await anchor1.setSigner(wallet1);
    const tokenAddress = signatureBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );

    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    // Create a deposit utxo
    const randomKeypair = new Keypair();
    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });

    // Make the deposit transaction
    await signatureBridge.transact([], [depositUtxo], 0, '0', '0', wallet1);

    // wait until the signature bridge recives the execute call.
    await webbRelayer.waitForEvent({
      kind: 'signature_bridge',
      event: { chain_id: localChain2.underlyingChainId.toString() },
    });
    // now we wait for the tx queue on that chain to execute the transaction.
    await webbRelayer.waitForEvent({
      kind: 'tx_queue',
      event: {
        ty: 'EVM',
        chain_id: localChain2.underlyingChainId.toString(),
        finalized: true,
      },
    });
    // all is good, last thing is to check for the roots.
    const srcChainRoot = await anchor1.contract.getLastRoot();
    const neigborRoots = await anchor2.contract.getLatestNeighborRoots();
    const edges = await anchor2.contract.getLatestNeighborEdges();
    const isKnownNeighborRoot = neigborRoots.some(
      (root: string) => root === srcChainRoot
    );
    if (!isKnownNeighborRoot) {
      console.log({
        srcChainRoot,
        neigborRoots,
        edges,
        isKnownNeighborRoot,
      });
    }
    expect(isKnownNeighborRoot).to.be.true;
  });

  after(async () => {
    await aliceNode?.stop();
    await bobNode?.stop();
    await charlieNode?.stop();
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});

describe.only('Signature Bridge <> Mocked Proposal Signing Backend', function () {
  const tmpDirPath = temp.mkdirSync();
  let localChain1: LocalChain;
  let localChain2: LocalChain;
  let signatureBridge: VBridge.VBridge;
  let wallet1: ethers.Wallet;
  let wallet2: ethers.Wallet;

  let webbRelayer: WebbRelayer;

  before(async () => {
    const PK1 = '0x0000000000000000000000000000000000000000000000000000000000000001';
    const PK2 = '0x0000000000000000000000000000000000000000000000000000000000000002';
    const localChain1Port = await getPort({
      port: portNumbers(3333, 4444),
    });

    const enabledContracts: EnabledContracts[] = [
      {
        contract: 'VAnchor',
      },
      {
        contract: 'SignatureBridge',
      }
    ];

    localChain1 = await LocalChain.init({
      port: localChain1Port,
      chainId: localChain1Port,
      name: 'Hermes',
      populatedAccounts: [
        {
          secretKey: PK1,
          balance: ethers.utils.parseEther('1000').toHexString(),
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
          balance: ethers.utils.parseEther('1000').toHexString(),
        },
      ],
      enabledContracts: enabledContracts,
    });

    wallet1 = new ethers.Wallet(PK1, localChain1.provider());
    wallet2 = new ethers.Wallet(PK2, localChain2.provider());
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

    console.log('deploying signature bridge...');
    signatureBridge = await localChain1.deploySignatureVBridge(
      localChain2,
      localToken1,
      localToken2,
      wallet1,
      wallet2,
    );
    console.log('Signature bridge deployed')
    // save the chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge: signatureBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
    });
    await localChain2.writeConfig(`${tmpDirPath}/${localChain2.name}.json`, {
      signatureVBridge: signatureBridge,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK2 },
    });

    // get the anhor on localchain1
    const anchor = signatureBridge.getVAnchor(
      localChain1.chainId,
    )!;
    await anchor.setSigner(wallet1);
    // approve token spending
    const tokenAddress = signatureBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    let tx = await token.approveSpending(anchor.contract.address);
    await tx.wait();
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));

    // do the same but on localchain2
    const anchor2 = signatureBridge.getVAnchor(
      localChain2.chainId,
    )!;
    await anchor2.setSigner(wallet2);
    const tokenAddress2 = signatureBridge.getWebbTokenAddress(
      localChain2.chainId
    )!;
    const token2 = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress2,
      wallet2
    );

    tx = await token2.approveSpending(anchor2.contract.address);
    await tx.wait();
    await token2.mintTokens(wallet2.address, ethers.utils.parseEther('1000'));

    // now start the relayer
    const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
    webbRelayer = new WebbRelayer({
      port: relayerPort,
      tmp: true,
      configDir: tmpDirPath,
      showLogs: true,
      verbosity: 3,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should handle AnchorUpdateProposal when a deposit happens using mocked proposal backend', async () => {
    // we will use chain1 as an example here.
    const anchor1 = signatureBridge.getVAnchor(
      localChain1.chainId,
    );
    const anchor2 = signatureBridge.getVAnchor(
      localChain2.chainId,
    );
    await anchor1.setSigner(wallet1);
    const tokenAddress = signatureBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    // Create a deposit utxo
    const randomKeypair = new Keypair();
    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: (1e2).toString(),
      originChainId: localChain1.chainId.toString(),
      chainId: localChain2.chainId.toString(),
      keypair: randomKeypair,
    });
    
    const bridgeSide = signatureBridge.getVBridgeSide(localChain1.chainId);
    console.log(bridgeSide.governor);
    const chainGovernor = await bridgeSide.contract.governor();
    console.log(chainGovernor);

    // Make the deposit transaction
    await signatureBridge.transact([], [depositUtxo], 0, '0', '0', wallet1);

    // wait until the signature bridge recives the execute call.
    await webbRelayer.waitForEvent({
      kind: 'signature_bridge',
      event: { chain_id: localChain2.underlyingChainId.toString() },
    });
    // now we wait for the tx queue on that chain to execute the transaction.
    await webbRelayer.waitForEvent({
      kind: 'tx_queue',
      event: {
        ty: 'EVM',
        chain_id: localChain2.underlyingChainId.toString(),
        finalized: true,
      },
    });
    // all is good, last thing is to check for the roots.
    const srcChainRoot = await anchor1.contract.getLastRoot();
    const neigborRoots = await anchor2.contract.getLatestNeighborRoots();
    const edges = await anchor2.contract.getLatestNeighborEdges();
    const isKnownNeighborRoot = neigborRoots.some(
      (root: string) => root === srcChainRoot
    );
    if (!isKnownNeighborRoot) {
      console.log({
        srcChainRoot,
        neigborRoots,
        edges,
        isKnownNeighborRoot,
      });
    }
    expect(isKnownNeighborRoot).to.be.true;
  });

  after(async () => {
    await localChain1?.stop();
    await localChain2?.stop();
    await webbRelayer?.stop();
  });
});
