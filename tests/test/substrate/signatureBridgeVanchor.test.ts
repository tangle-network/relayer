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
// This is Substrate VAnchor Transaction Relayer Tests.
// In this test relayer on vanchor deposit will create and relay proposals to signature bridge pallet for execution

import '@webb-tools/protocol-substrate-types';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import isCi from 'is-ci';
import child from 'child_process';
import { ethers } from 'ethers';
import {
  WebbRelayer,
  Pallet,
  RelayerMetricResponse,
} from '../../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../../lib/localProtocolSubstrate.js';
import { SubmittableExtrinsic } from '@polkadot/api/types';

import { BigNumber } from 'ethers';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import { naclEncrypt, randomAsU8a } from '@polkadot/util-crypto';

import {
  ProvingManagerSetupInput,
  ArkworksProvingManager,
  Utxo,
  calculateTypedChainId,
  ChainType,
  toFixedHex,
  ResourceId,
  ProposalHeader,
  LeafIdentifier,
} from '@webb-tools/sdk-core';

import {
  encodeResourceIdUpdateProposal,
  SubstrateResourceIdUpdateProposal,
} from '../../lib/substrateWebbProposals.js';
import pkg from 'secp256k1';
import { makeSubstrateTargetSystem } from '../../lib/webbProposals.js';
import {
  defaultEventsWatcherValue,
  generateVAnchorNote,
} from '../../lib/utils.js';
import { UsageMode } from '@webb-tools/test-utils';
import { expect } from 'chai';
const { ecdsaSign } = pkg;

describe('Substrate Signature Bridge Relaying On Vanchor Deposit <<>> Mocked Backend', function () {
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;
  let webbRelayer: WebbRelayer;

  // Governer key
  const PK1 = u8aToHex(ethers.utils.randomBytes(32));
  const governorWallet = new ethers.Wallet(PK1);
  // slice 0x04 from public key
  const uncompressedKey = governorWallet
    ._signingKey()
    .publicKey.toString()
    .slice(4);
  const typedSourceChainId = calculateTypedChainId(ChainType.Substrate, 1080);

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

    const chainId = await aliceNode.getChainId();
    await aliceNode.writeConfig(`${tmpDirPath}/${aliceNode.name}.json`, {
      suri: '//Charlie',
      chainId: chainId,
      proposalSigningBackend: { type: 'Mocked', privateKey: PK1 },
      linkedAnchors: [{ type: 'Substrate', treeId: 6, chainId, pallet: 44 }],
      enabledPallets,
    });

    // force set maintainer
    const refreshNonce = 0;
    const setMaintainerCall = api.tx.signatureBridge.forceSetMaintainer(
      refreshNonce,
      `0x${uncompressedKey}`
    );
    await aliceNode.sudoExecuteTransaction(setMaintainerCall);

    //whitelist chain
    const whitelistChainCall =
      api.tx.signatureBridge.whitelistChain(typedSourceChainId);
    await aliceNode.sudoExecuteTransaction(whitelistChainCall);

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

  it('Relayer should create and relay anchor update proposal to signature bridge for execution', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Dave');
    //create vanchor
    const createVAnchorCall = api.tx.vAnchorBn254.create(1, 30, 0);
    await aliceNode.sudoExecuteTransaction(createVAnchorCall);

    const nextTreeId = await api.query.merkleTreeBn254.nextTreeId();
    const treeId = nextTreeId.toNumber() - 1;

    // chainId
    const chainId = await aliceNode.getChainId();
    console.log('step1');
    // now we set resource through proposal execution
    const setResourceIdProposalCall = await setResourceIdProposal(
      api,
      PK1,
      treeId,
      chainId
    );
    const txSigned = await setResourceIdProposalCall.signAsync(account);
    await aliceNode.executeTransaction(txSigned);

    // vanchor deposit
    await vanchorDeposit(treeId, api, aliceNode);

    // now we wait for the proposal to be signed by mocked backend and then send data to signature bridge
    await webbRelayer.waitForEvent({
      kind: 'signing_backend',
      event: {
        backend: 'Mocked',
      },
    });

    // now we wait for proposals to be verified and executed by signature bridge through transaction queue.

    await webbRelayer.waitForEvent({
      kind: 'tx_queue',
      event: {
        ty: 'SUBSTRATE',
        chain_id: chainId.toString(),
        finalized: true,
      },
    });

    // check metrics gathered
    const responseMetricsGathered = await webbRelayer.getMetricsGathered();
    expect(responseMetricsGathered.status).equal(200);
    let metricsGathered =
      responseMetricsGathered.json() as Promise<RelayerMetricResponse>;
    metricsGathered.then((resp) => {
      console.log(resp.metrics);
      expect(resp.metrics).to.not.be.null;
    });
  });

  after(async () => {
    await aliceNode?.stop();
    await bobNode?.stop();
    await webbRelayer?.stop();
  });
});

// Helper methods, we can move them somewhere if we end up using them again.

async function setResourceIdProposal(
  api: ApiPromise,
  PK1: string,
  treeId: number,
  chainId: number
): Promise<SubmittableExtrinsic<'promise'>> {
  const functionSignature = hexToU8a('0x00000002', 32);
  const nonce = 1;
  const palletIndex = '0x2C';
  const callIndex = '0x02';
  const substrateTargetSystem = makeSubstrateTargetSystem(treeId, palletIndex);
  // set resource ID
  const resourceId = new ResourceId(
    toFixedHex(substrateTargetSystem, 20),
    ChainType.Substrate,
    chainId
  );
  const proposalHeader = new ProposalHeader(
    resourceId,
    functionSignature,
    nonce
  );
  const resourceIdUpdateProposal: SubstrateResourceIdUpdateProposal = {
    header: proposalHeader,
    newResourceId: resourceId.toString(),
    palletIndex,
    callIndex,
  };

  const proposalBytes = encodeResourceIdUpdateProposal(
    resourceIdUpdateProposal
  );
  const hash = ethers.utils.keccak256(proposalBytes);
  const msg = ethers.utils.arrayify(hash);
  // sign the message
  const sigObj = ecdsaSign(msg, hexToU8a(PK1));
  const signature = new Uint8Array([...sigObj.signature, sigObj.recid]);

  const setResourceCall = api.tx.signatureBridge.setResourceWithSignature(
    calculateTypedChainId(ChainType.Substrate, chainId),
    u8aToHex(proposalBytes),
    u8aToHex(signature)
  );
  return setResourceCall;
}

async function vanchorDeposit(
  treeId: number,
  api: ApiPromise,
  aliceNode: LocalProtocolSubstrate
) {
  const account = createAccount('//Dave');
  const chainId = '2199023256632';
  const outputChainId = BigInt(chainId);
  const secret = randomAsU8a();
  const gitRoot = child
    .execSync('git rev-parse --show-toplevel')
    .toString()
    .trim();

  const pkPath = path.join(
    // tests path
    gitRoot,
    'tests',
    'substrate-fixtures',
    'vanchor',
    'bn254',
    'x5',
    '2-2-2',
    'proving_key_uncompressed.bin'
  );
  const pk_hex = fs.readFileSync(pkPath).toString('hex');
  const pk = hexToU8a(pk_hex);

  // Creating two empty vanchor notes
  const note1 = await generateVAnchorNote(
    0,
    Number(outputChainId.toString()),
    Number(outputChainId.toString()),
    0
  );
  const note2 = await note1.getDefaultUtxoNote();
  const notes = [note1, note2];
  const publicAmount = currencyToUnitI128(10);
  // Output UTXOs configs
  const output1 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: publicAmount.toString(),
    chainId,
  });
  const output2 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: '0',
    chainId,
  });

  // Configure a new proving manager with direct call
  const provingManager = new ArkworksProvingManager(null);
  const leavesMap = {};

  const address = account.address;
  const extAmount = currencyToUnitI128(10);
  const fee = 0;
  const refund = 0;
  const assetId = new Uint8Array([0, 0, 0, 0]); // WEBB native token asset Id.
  // Empty leaves
  leavesMap[outputChainId.toString()] = [];
  const tree = await api.query.merkleTreeBn254.trees(treeId);
  const root = tree.unwrap().root.toHex();
  const rootsSet = [hexToU8a(root), hexToU8a(root)];
  const decodedAddress = decodeAddress(address);
  const { encrypted: comEnc1 } = naclEncrypt(output1.commitment, secret);
  const { encrypted: comEnc2 } = naclEncrypt(output2.commitment, secret);
  const leafId: LeafIdentifier = {
    index: 0,
    typedChainId: Number(outputChainId.toString()),
  };

  const setup: ProvingManagerSetupInput<'vanchor'> = {
    chainId: outputChainId.toString(),
    leafIds: [leafId, leafId],
    inputUtxos: notes.map((n) => new Utxo(n.note.getUtxo())),
    leavesMap: leavesMap,
    output: [output1, output2],
    encryptedCommitments: [comEnc1, comEnc2],
    provingKey: pk,
    publicAmount: String(publicAmount),
    roots: rootsSet,
    relayer: decodedAddress,
    recipient: decodedAddress,
    extAmount: extAmount.toString(),
    fee: fee.toString(),
    refund: String(refund),
    token: assetId,
  };

  const data = await provingManager.prove('vanchor', setup);
  const extData = {
    relayer: address,
    recipient: address,
    fee,
    refund: String(refund),
    token: assetId,
    extAmount: extAmount.toNumber(),
    encryptedOutput1: u8aToHex(comEnc1),
    encryptedOutput2: u8aToHex(comEnc2),
  };

  const vanchorProofData = {
    proof: `0x${data.proof}`,
    publicAmount: data.publicAmount,
    roots: rootsSet,
    inputNullifiers: data.inputUtxos.map((input) => `0x${input.nullifier}`),
    outputCommitments: data.outputUtxos.map((utxo) => utxo.commitment),
    extDataHash: data.extDataHash,
  };

  // now we call the vanchor transact
  const transactCall = api.tx.vAnchorBn254.transact(
    treeId,
    vanchorProofData,
    extData
  );
  const txSigned = await transactCall.signAsync(account);
  await aliceNode.executeTransaction(txSigned);
}

function currencyToUnitI128(currencyAmount: number) {
  const bn = BigNumber.from(currencyAmount);
  return bn.mul(1_000_000_000_000);
}

function createAccount(accountId: string) {
  const keyring = new Keyring({ type: 'sr25519' });
  const account = keyring.addFromUri(accountId);

  return account;
}
