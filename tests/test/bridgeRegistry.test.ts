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
// This is evm to substrate cross transaction relayer tests.
// In this test we will deposit on evm vanchor system
// and withdraw through substrate vanchor system.

import '@webb-tools/protocol-substrate-types';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import isCi from 'is-ci';
import retry from 'async-retry';
import { ethers } from 'ethers';
import {
  WebbRelayer,
  Pallet,
  EnabledContracts,
} from '../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../lib/localProtocolSubstrate.js';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';

import {
  calculateTypedChainId,
  ChainType,
  ProposalHeader,
  CircomUtxo,
  AnchorUpdateProposal,
  ResourceId,
} from '@webb-tools/sdk-core';



import { createSubstrateResourceId } from '../lib/webbProposals.js';
import { LocalChain } from '../lib/localTestnet.js';
import { Tokens, VBridge } from '@webb-tools/protocol-solidity';
import { expect } from 'chai';
import {
  defaultEventsWatcherValue,
} from '../lib/utils.js';
import { currencyToUnitI128, UsageMode } from '@webb-tools/test-utils';
import { LocalDkg } from '../lib/localDkg.js';
import { KeyringPair } from '@polkadot/keyring/types.js';
import { ethAddressFromUncompressedPublicKey } from '../lib/ethHelperFunctions.js';
import { MintableToken } from '@webb-tools/tokens';

// These are for testing bridge registry pallet integration test for substrate chains.
// In this test relayer will directly fetch linked anchor config from bridge registry pallet.

describe.skip('Bridge Registry Pallet Integration Test <=> Substrate', function () {
  const tmpDirPath = temp.mkdirSync();
  // Evm node
  let localChain1: LocalChain;

  // Protocol-substrate node
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;

  // dkg nodes
  let dkgNode1: LocalDkg;
  let dkgNode2: LocalDkg;
  let dkgNode3: LocalDkg;

  let webbRelayer: WebbRelayer;
  let wallet1: ethers.Wallet;
  let relayerExternalWallet: ethers.Wallet;
  let relayerNativeWallet: KeyringPair;
  let signatureVBridge: VBridge.VBridge;

  // substrate vanchor treeId
  let treeId: number;

  const PK1 = u8aToHex(ethers.utils.randomBytes(32));
  const relayerPK = u8aToHex(ethers.utils.randomBytes(32));


  before(async () => {
    const usageModeDkg: UsageMode = isCi
      ? { mode: 'host', nodePath: 'dkg-standalone-node' }
      : {
        mode: 'host',
        nodePath: path.resolve(
          '../../dkg-substrate/target/release/dkg-standalone-node'
        ),
      };

    const dkgEnabledPallets: Pallet[] = [
      {
        pallet: 'DKGProposalHandler',
        eventsWatcher: defaultEventsWatcherValue,
      },
      {
        pallet: 'DKG',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];

    // Step 1. We initialize DKG nodes.
    dkgNode1 = await LocalDkg.start({
      name: 'dkg-alice',
      authority: 'alice',
      usageMode: usageModeDkg,
      ports: 'auto',
      enableLogging: false,
    });

    dkgNode2 = await LocalDkg.start({
      name: 'dkg-bob',
      authority: 'bob',
      usageMode: usageModeDkg,
      ports: 'auto',
      enableLogging: false,
    });

    dkgNode3 = await LocalDkg.start({
      name: 'dkg-charlie',
      authority: 'charlie',
      usageMode: usageModeDkg,
      ports: 'auto',
      enableLogging: false,
    });

    // Wait until we are ready and connected
    const dkgApi = await dkgNode3.api();
    await dkgApi.isReady;
    console.log('dkg node ready');
    const dkgNodeChainId = await dkgNode3.getChainId();
    // Step 2. We need to wait until the public key is on chain.
    await dkgNode3.waitForEvent({
      section: 'dkg',
      method: 'PublicKeySignatureChanged',
    });


    // We start protocol-substrate nodes.
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
      {
        pallet: 'SignatureBridge',
        eventsWatcher: defaultEventsWatcherValue,
      },
    ];

    aliceNode = await LocalProtocolSubstrate.start({
      name: 'substrate-alice',
      authority: 'alice',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });

    bobNode = await LocalProtocolSubstrate.start({
      name: 'substrate-bob',
      authority: 'bob',
      usageMode,
      ports: 'auto',
      enableLogging: false,
    });
    // Wait until we are ready and connected
    const api = await aliceNode.api();
    await api.isReady;
    relayerNativeWallet = createAccount("//Charlie");
    const substrateChainId = await aliceNode.getChainId();
    // Create vanchor on Substrate chain with height 30 and maxEdges = 1
    const createVAnchorCall = api.tx.vAnchorBn254.create(1, 30, 0);
    await aliceNode.sudoExecuteTransaction(createVAnchorCall);
    const nextTreeId = await api.query.merkleTreeBn254.nextTreeId();
    treeId = nextTreeId.toNumber() - 1;
    const substrateResourceId = createSubstrateResourceId(
      substrateChainId,
      treeId,
      '0x2C'
    );
    await substrateSetup(aliceNode, api, treeId, substrateChainId, substrateResourceId);
    console.log('substrate node ready');

    // Start Evm chain node
    const localChain1Port = await getPort({
      port: portNumbers(3333, 4444),
    });

    const enabledContracts: EnabledContracts[] = [
      {
        contract: 'VAnchor',
      },

      {
        contract: 'SignatureBridge'
      }
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
        {
          secretKey: relayerPK,
          balance: ethers.utils
            .parseEther('100000000000000000000000')
            .toHexString(),
        }

      ],
      enabledContracts: enabledContracts,
    });
    wallet1 = new ethers.Wallet(PK1, localChain1.provider());
    // Deploy the token.
    const localToken = await localChain1.deployToken('Webb Token', 'WEBB');
    const unwrappedToken = await MintableToken.createToken(
      'Webb Token',
      'WEBB',
      wallet1
    );

    signatureVBridge = await localChain1.deployVBridge(
      localToken,
      unwrappedToken,
      wallet1,
      wallet1
    );
    // Get the anhor on localchain1
    const vanchor = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor.setSigner(wallet1);
    const evmResourceId = await vanchor.createResourceId();
    // relayer external wallet
    relayerExternalWallet = new ethers.Wallet(relayerPK, localChain1.provider());
    console.log('Local evm chain ready');


    await dkgSetup(
      dkgNode3,
      dkgApi,
      substrateChainId,
      localChain1.underlyingChainId,
      substrateResourceId.toString(),
      evmResourceId,
      relayerExternalWallet,
      relayerNativeWallet
    );
    // Fetch current active governor from dkg node
    const dkgPublicKey = await dkgNode3.fetchDkgPublicKey();
    expect(dkgPublicKey).to.not.be.null;
    await transferOwnsershipSubstrate(aliceNode, api, dkgPublicKey!);
    await transferOwnershipEvm(signatureVBridge, dkgPublicKey!);
    
    // Send anchor update proposal to register bride and its resources on DKG node.
    await sendAnchorUpdateProposal(
      dkgNode3,
      dkgApi,
      evmResourceId,
      substrateResourceId.toString()
      );

    // Save substrate node chain configs
    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      chainId: substrateChainId,
      proposalSigningBackend: { type: 'DKGNode', chainId: dkgNodeChainId },
      enabledPallets,
    });
    // save evm node chain configs.
    await localChain1.writeConfig(`${tmpDirPath}/${localChain1.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: { type: 'DKGNode', chainId: dkgNodeChainId },
      privateKey: relayerPK,
    });
    // save dkg node chain confis
    await dkgNode3.writeConfig(`${tmpDirPath}/${dkgNode3.name}.json`, {
      suri: '//Charlie',
      chainId: dkgNodeChainId,
      enabledPallets: dkgEnabledPallets,
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

  it('should relay roots to linked anchors', async () => {
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    await vanchor1.setSigner(wallet1);
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    const tokenAddress = signatureVBridge.getWebbTokenAddress(
      localChain1.chainId
    )!;
    const token = await Tokens.MintableToken.tokenFromAddress(
      tokenAddress,
      wallet1
    );

    const tx = await token.approveSpending(
      vanchor1.contract.address,
      ethers.utils.parseEther('1000')
    );
    await tx.wait();

    // Mint 1000 * 10^18 tokens to wallet1
    await token.mintTokens(wallet1.address, ethers.utils.parseEther('1000'));
    const webbBalance = await token.getBalance(wallet1.address);
    expect(webbBalance.toBigInt() > ethers.utils.parseEther('1').toBigInt()).to
      .be.true;

    const substrateChainId = await aliceNode.getChainId();
    const typedTargetChainId = calculateTypedChainId(
      ChainType.Substrate,
      substrateChainId
    );
    const typedSourceChainId = localChain1.chainId;
    
    // deposit amount on evm
    const publicAmount = currencyToUnitI128(10);
    const depositUtxo = await CircomUtxo.generateUtxo({
      curve: 'Bn254',
      backend: 'Circom',
      amount: publicAmount.toString(),
      originChainId: typedSourceChainId.toString(),
      chainId: typedTargetChainId.toString(),
    });

    const leaves = vanchor1.tree
      .elements()
      .map((el) => hexToU8a(el.toHexString()));

    await vanchor1.transact([], [depositUtxo], 0, 0, '0', '0', tokenAddress, {
      [typedSourceChainId]: leaves,
    });
    console.log("Deposit made");
    
    // now we wait for the proposal to be signed by mocked backend and then send data to signature bridge
    await webbRelayer.waitForEvent({
      kind: 'signing_backend',
      event: {
        backend: 'DKG',
      },
    });

    // now we wait for proposals to be verified and executed by signature bridge through transaction queue.
    await webbRelayer.waitForEvent({
      kind: 'tx_queue',
      event: {
        ty: 'SUBSTRATE',
        chain_id: substrateChainId.toString(),
        finalized: true,
      },
    });
  });

  after(async () => {
    await localChain1?.stop();
    await aliceNode?.stop();
    await bobNode?.stop();
    await dkgNode1?.stop();
    await dkgNode2?.stop();
    await dkgNode3?.stop();
    await webbRelayer?.stop();
  });
});

// Helper methods, we can move them somewhere if we end up using them again.

function createAccount(accountId: string): KeyringPair {
  const keyring = new Keyring({ type: 'sr25519' });
  const account = keyring.addFromUri(accountId);
  return account;
}

async function transferOwnsershipSubstrate(
  aliceNode: LocalProtocolSubstrate,
  api: ApiPromise, dkgPublicKey: `0x${string}`) {
  // force set maintainer
  const refreshNonce = 0;
  const setMaintainerCall = api.tx.signatureBridge.forceSetMaintainer(
    refreshNonce,
    dkgPublicKey
  );
  await aliceNode.sudoExecuteTransaction(setMaintainerCall);

}

async function transferOwnershipEvm(
  signatureVBridge: VBridge.VBridge,
  dkgPublicKey: `0x${string}`) {

  const governorAddress = ethAddressFromUncompressedPublicKey(dkgPublicKey);
  // verify the governor address is a valid ethereum address.
  expect(ethers.utils.isAddress(governorAddress)).to.be.true;
  // transfer ownership to the DKG.
  const sides = signatureVBridge.vBridgeSides.values();
  for (const signatureSide of sides) {
    // now we transferOwnership, forcefully.
    const tx = await signatureSide.transferOwnership(governorAddress, 1);
    await retry(
      async () => {
        await tx.wait();
      },
      {
        retries: 3,
        minTimeout: 500,
        onRetry: (error) => {
          console.error('transferOwnership retry', error.name, error.message);
        },
      }
    );
    // check that the new governor is the same as the one we just set.
    const currentGovernor = await signatureSide.contract.governor();
    expect(currentGovernor).to.eq(governorAddress);
  }
}

// Setup substrate chain, we will whitelist chains and register resourceId
async function substrateSetup(
  aliceNode: LocalProtocolSubstrate,
  api: ApiPromise,
  treeId: number,
  substrateChainId: number,
  substrateResourceId: ResourceId
) {
  // Whitelist chain on substrate node
  const whitelistChainCall =
    api.tx.signatureBridge.whitelistChain(substrateChainId);
  await aliceNode.sudoExecuteTransaction(whitelistChainCall);
  // Set resource on signature bridge
  const setResourceCall = api.tx.signatureBridge.setResource(substrateResourceId.toU8a());
  await aliceNode.sudoExecuteTransaction(setResourceCall);
  // set resource on vanchor-handler
  const forceSetResource = api.tx.vAnchorHandlerBn254.forceSetResource!(substrateResourceId.toU8a(), treeId);
  await aliceNode.sudoExecuteTransaction(forceSetResource);

}

async function dkgSetup(
  dkgNode: LocalDkg,
  dkgApi: ApiPromise,
  substrateChainId: number,
  evmChainId: number,
  substrateResourceId: string,
  evmResourceId: string,
  relayerExternalWallet: ethers.Wallet,
  relayerNativeWallet: KeyringPair

) {
  // Whitelist chain in dkg node
  const whitelistSubstrateChainDkgCall = dkgApi.tx.dkgProposals.whitelistChain({ Substrate: substrateChainId });
  await dkgNode.sudoExecuteTransaction(whitelistSubstrateChainDkgCall);

  const whitelistEVMChainDkgCall = dkgApi.tx.dkgProposals.whitelistChain({ Evm: evmChainId });
  await dkgNode.sudoExecuteTransaction(whitelistEVMChainDkgCall);

  // Set resources
  const setResource1Call = dkgApi.tx.dkgProposals.setResource(evmResourceId, '0x')
  await dkgNode.sudoExecuteTransaction(setResource1Call);
  const setResource2Call = dkgApi.tx.dkgProposals.setResource(substrateResourceId, '0x')
  await dkgNode.sudoExecuteTransaction(setResource2Call);

  // Add Proposer
  const addProposerCall = dkgApi.tx.dkgProposals.addProposer(relayerNativeWallet.address, relayerExternalWallet.address);
  await dkgNode.sudoExecuteTransaction(addProposerCall);
}

async function sendAnchorUpdateProposal(
  dkgNode: LocalDkg,
  dkgApi: ApiPromise,
  evmResourceId: string,
  substrateResourceId: string
) {
  // send dummy proposal to register bridge
  const sourceResourceId = ResourceId.fromBytes(hexToU8a(evmResourceId));
  const targetResourceId = ResourceId.fromBytes(hexToU8a(substrateResourceId));
  const functionSignature = hexToU8a('0x00000002', 32);
  const merkleRoot = u8aToHex(ethers.utils.randomBytes(32));
  const header = new ProposalHeader(targetResourceId, functionSignature, 0)
  const anchorUpdateProposal = new AnchorUpdateProposal(header, merkleRoot, sourceResourceId);
  const kind = dkgApi.createType(
    'WebbProposalsProposalProposalKind',
    'AnchorUpdate'
  );
  const prop = dkgApi.createType('WebbProposalsProposal', {
    Unsigned: {
      kind,
      data: u8aToHex(anchorUpdateProposal.toU8a()),
    },
  });

  const submitUnsignedProposalCall = dkgApi.tx.dkgProposalHandler.forceSubmitUnsignedProposal(prop.toU8a());
  await dkgNode.sudoExecuteTransaction(submitUnsignedProposalCall);
  console.log("Dummy proposal sent");
}