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
// This our basic Substrate VAnchor Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just to relay transactions for us.

import '@webb-tools/protocol-substrate-types';
import { assert, expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import isCi from 'is-ci';
import child from 'child_process';
import {
  WebbRelayer,
  Pallet,
  LeavesCacheResponse,
} from '../../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../../lib/localProtocolSubstrate.js';

import { BigNumber, ethers } from 'ethers';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { decodeAddress } from '@polkadot/util-crypto';
import { naclEncrypt, randomAsU8a } from '@polkadot/util-crypto';

import {
  ProvingManagerSetupInput,
  ArkworksProvingManager,
  Utxo,
  VAnchorProof,
  LeafIdentifier,
  calculateTypedChainId,
  ChainType,
  Note,
  Keypair,
  toFixedHex,
} from '@webb-tools/sdk-core';
import { UsageMode } from '@webb-tools/test-utils';
import {
  defaultEventsWatcherValue,
  generateVAnchorNote,
} from '../../lib/utils.js';

describe('Substrate VAnchor Private Transaction Relayer Tests', function () {
  const tmpDirPath = temp.mkdirSync();
  let aliceNode: LocalProtocolSubstrate;
  let bobNode: LocalProtocolSubstrate;

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
      showLogs: true,
    });
    await webbRelayer.waitUntilReady();
  });

  it('should relay private transaction ', async () => {
    const api = await aliceNode.api();
    const account = createAccount('//Dave');
    // Create vanchor on Substrate chain with height 30 and maxEdges = 1
    const createVAnchorCall = api.tx.vAnchorBn254.create(1, 30, 0);
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
    
     // dummy Deposit Note. Input note is directed toward source chain
     const depositNote = await generateVAnchorNote(
      0,
      typedSourceChainId,
      typedSourceChainId,
      0
    );
    
    // substrate vanchor deposit
    const data = await vanchorDeposit(
      typedSourceChainId.toString(),
      depositNote,
      // public amount
      10,
      treeId,
      api,
      aliceNode
    );

    console.log("deposit made");

    // now we wait for all deposit to be saved in LeafStorageCache.
    await webbRelayer.waitForEvent({
      kind: 'leaves_store',
      event: {
        leaf_index: 2,
      },
    });

    console.log("Vnahcor withdraw");

    
    const vanchorData = await vanchorWithdraw(
      typedSourceChainId.toString(),
      typedSourceChainId.toString(),
      data.depositUtxos,
      treeId,
      api,
      aliceNode
    )
    
    // now we withdraw using private transaction
    const txHash = await webbRelayer.substrateVAnchorWithdraw(
      substrateChainId,
      treeId,
      vanchorData.extData,
      vanchorData.vanchorProofData
      )

    // now we wait for relayer to execute private transaction.
    await webbRelayer.waitForEvent({
        kind: 'private_tx',
        event: {
          ty: 'SUBSTRATE',
          chain_id: substrateChainId.toString(),
          finalized: true,
        },
      });
  });

  after(async () => {
    await aliceNode?.stop();
    await bobNode?.stop();
    await webbRelayer?.stop();
  });
});

// Helper methods, we can move them somewhere if we end up using them again.

function currencyToUnitI128(currencyAmount: number) {
  const bn = BigNumber.from(currencyAmount);
  return bn.mul(1_000_000_000_000);
}

function createAccount(accountId: string): any {
  const keyring = new Keyring({ type: 'sr25519' });
  const account = keyring.addFromUri(accountId);

  return account;
}


async function vanchorWithdraw(
  typedTargetChainId: string,
  typedSourceChainId: string,
  depositUtxos:[Utxo, Utxo],
  treeId: number,
  api: ApiPromise,
  aliceNode: LocalProtocolSubstrate
): Promise<{ extData: any; vanchorProofData: any }> {
  const account = createAccount('//Dave');
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
  const leavesMap = {};
  // get source chain (evm) leaves.
  //@ts-ignore
  const substrateLeaves = await api.derive.merkleTreeBn254.getLeavesForTree(
    treeId,
    0,
    1
  );
  assert(substrateLeaves.length === 2, 'Invalid substrate leaves length');
  substrateLeaves.map(
    (leaf, index) =>  {
      if(depositUtxos[0].commitment.toString() === leaf.toString()){
        depositUtxos[0].setIndex(index)
      }
      else if(depositUtxos[1].commitment.toString() === leaf.toString()){
        depositUtxos[1].setIndex(index)
      }
  }
  );
  leavesMap[typedSourceChainId.toString()] = substrateLeaves;

  // Output UTXOs configs
  const output1 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: '0',
    chainId: typedSourceChainId
  });
  const output2 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: '0',
    chainId: typedSourceChainId
  });

  // Configure a new proving manager with direct call
  const provingManager = new ArkworksProvingManager(null);
  const assetId = new Uint8Array([254, 255, 255, 255]);
  const address = account.address;
  const fee = 0;
  const refund = 0;
  const withdrawAmount = depositUtxos.reduce((acc, utxo) => {
    return Number(utxo.amount) + acc;
  }, 0);
  const extAmount = -withdrawAmount;

  const publicAmount = -withdrawAmount;
 
  const tree = await api.query.merkleTreeBn254.trees(treeId);
  const root = tree.unwrap().root.toHex();
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore TODO: Fix the issue with rpc type augmentation
  const neighborRoots: string[] = await api.rpc.lt
    .getNeighborRoots(treeId)
    .then((roots) => roots.toHuman());
  const rootsSet = [hexToU8a(root), hexToU8a(neighborRoots[0])];
  const decodedAddress = decodeAddress(address);
  const { encrypted: comEnc1 } = naclEncrypt(output1.commitment, secret);
  const { encrypted: comEnc2 } = naclEncrypt(output2.commitment, secret);

  const setup: ProvingManagerSetupInput<'vanchor'> = {
    chainId: typedTargetChainId.toString(),
    leafIds: depositUtxos.map((utxo) => {
      return {
        index: utxo.index!,
        typedChainId: Number(utxo.chainId)
      } 
    }),
    inputUtxos: depositUtxos,
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
    extAmount: extAmount,
    encryptedOutput1: u8aToHex(comEnc1),
    encryptedOutput2: u8aToHex(comEnc2),
  };
  let vanchorProofData = {
    proof: `0x${data.proof}`,
    publicAmount: data.publicAmount,
    roots: rootsSet,
    inputNullifiers: data.inputUtxos.map((input) => `0x${input.nullifier}`),
    outputCommitments: data.outputUtxos.map((utxo) => utxo.commitment),
    extDataHash: data.extDataHash,
  };

  // now we call the vanchor transact to withdraw on substrate
  let transactCall = api.tx.vAnchorBn254!.transact!(
    treeId,
    vanchorProofData,
    extData
  );
  const txSigned = await transactCall.signAsync(account);
  await aliceNode.executeTransaction(txSigned);
  return {extData, vanchorProofData}
}

async function vanchorDeposit(
  typedTargetChainId: string,
  depositNote: Note,
  publicAmountUint: number,
  treeId: number,
  api: ApiPromise,
  aliceNode: LocalProtocolSubstrate
): Promise<{ depositUtxos: [Utxo , Utxo]}> {
  const account = createAccount('//Dave');
  const typedSourceChainId = depositNote.note.sourceChainId;
  const secret = randomAsU8a();
  // Key pair for deposit UTXO for spend
  const randomKeypair = new Keypair();
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

  const vkPath = path.join(
    // tests path
    gitRoot,
    'tests',
    'substrate-fixtures',
    'vanchor',
    'bn254',
    'x5',
    '2-2-2',
    'verifying_key_uncompressed.bin'
  );
  const vk_hex = fs.readFileSync(vkPath).toString('hex');
  const vk = hexToU8a(vk_hex);

  const note1 = depositNote;
  const note2 = await note1.getDefaultUtxoNote();
  const publicAmount = currencyToUnitI128(publicAmountUint);
  const notes = [note1, note2];
  // Output UTXOs configs
  const output1 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: publicAmount.toString(),
    chainId: typedTargetChainId,
  });
  const output2 = await Utxo.generateUtxo({
    curve: 'Bn254',
    backend: 'Arkworks',
    amount: '0',
    chainId: typedTargetChainId,
  });

  // Configure a new proving manager with direct call
  const provingManager = new ArkworksProvingManager(null);
  const leavesMap = {};

  const address = account.address;
  const extAmount = currencyToUnitI128(10);
  const fee = 0;
  const refund = 0;
  // Initially leaves will be empty
  leavesMap[typedTargetChainId.toString()] = [];
  const tree = await api.query.merkleTreeBn254.trees(treeId);
  const root = tree.unwrap().root.toHex();
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  const neighborRoots: string[] = await api.rpc.lt
    .getNeighborRoots(treeId)
    .then((roots) => roots.toHuman());

  const rootsSet = [hexToU8a(root), hexToU8a(neighborRoots[0])];
  const decodedAddress = decodeAddress(address);
  const assetId = new Uint8Array([254, 255, 255, 255]);
  const { encrypted: comEnc1 } = naclEncrypt(output1.commitment, secret);
  const { encrypted: comEnc2 } = naclEncrypt(output2.commitment, secret);
  const LeafId = {
    index: 0,
    typedChainId: Number(typedSourceChainId),
  };
  const setup: ProvingManagerSetupInput<'vanchor'> = {
    chainId: typedSourceChainId.toString(),
    inputUtxos: notes.map((n) => new Utxo(n.note.getUtxo())),
    leafIds: [LeafId, LeafId],
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
    extAmount: extAmount,
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

  // now we call the vanchor transact to deposit on substrate.
  const transactCall = api.tx.vAnchorBn254.transact(
    treeId,
    vanchorProofData,
    extData
  );
  const txSigned = await transactCall.signAsync(account);
  await aliceNode.executeTransaction(txSigned);
  return { depositUtxos: data.outputUtxos };
}

