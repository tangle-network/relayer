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
// This our basic Substrate Anchor Transaction Relayer Tests.
// These are for testing the basic relayer functionality. which is just to relay transactions for us.

import { expect } from 'chai';
import getPort, { portNumbers } from 'get-port';
import temp from 'temp';
import path from 'path';
import fs from 'fs';
import isCi from 'is-ci';
import child from 'child_process';
import { WebbRelayer } from '../../lib/webbRelayer.js';
import { LocalProtocolSubstrate } from '../../lib/localProtocolSubstrate.js';
import { UsageMode } from '../../lib/substrateNodeBase.js';
import { ApiPromise, Keyring } from '@polkadot/api';
import { u8aToHex, hexToU8a } from '@polkadot/util';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { decodeAddress } from '@polkadot/util-crypto';
import {
    Note,
    NoteGenInput,
    ProvingManagerSetupInput,
    ProvingManagerWrapper,
} from '@webb-tools/sdk-core';

describe('Substrate Anchor Transaction Relayer', function () {
    this.timeout(60000);
    const tmpDirPath = temp.mkdirSync();
    let aliceNode: LocalProtocolSubstrate;
    let bobNode: LocalProtocolSubstrate;

    let webbRelayer: WebbRelayer;

    before(async () => {
        const usageMode: UsageMode = isCi
            ? { mode: 'docker', forcePullImage: false }
            : {
                mode: 'host',
                nodePath: path.resolve(
                    '../../protocol-substrate/target/release/webb-standalone-node'
                ),
            };

        // for manual connection
        const aliceManualPorts = {
            ws: 9944,
            http: 9933,
            p2p: 30333
        }

        // for manual connection
        const bobManualPorts = {
            ws: 9945,
            http: 9934,
            p2p: 30334
        }

        aliceNode = await LocalProtocolSubstrate.start({
            name: 'substrate-alice',
            authority: 'alice',
            usageMode,
            ports: aliceManualPorts,
            isManual: true,
            enableLogging: true
        });

        bobNode = await LocalProtocolSubstrate.start({
            name: 'substrate-bob',
            authority: 'bob',
            usageMode,
            ports: bobManualPorts,
            isManual: true,
            enableLogging: true
        });

        await aliceNode.writeConfig({
            path: `${tmpDirPath}/${aliceNode.name}.json`,
            suri: '//Charlie',
        });

        // now start the relayer
        const relayerPort = await getPort({ port: portNumbers(8000, 8888) });
        webbRelayer = new WebbRelayer({
            port: relayerPort,
            tmp: true,
            configDir: tmpDirPath,
            showLogs: true,
        });
        await webbRelayer.waitUntilReady();
    });

    it('Simple Anchor Transaction', async () => {
        console.log("inside simple anchor transaction");

        const api = await aliceNode.api();
        const account = createAccount('//Dave');
        const note = await makeDeposit(api, aliceNode, account);
        const withdrawalProof = await initWithdrawal(
            api,
            webbRelayer,
            account,
            note
        );

        // get the initial balance
        // @ts-ignore
        let { nonce, data: balance } = await api.query.system.account(
            withdrawalProof.recipient
        );
        let initialBalance = balance.free.toBigInt();
        console.log(`balance before withdrawal is ${balance.free.toBigInt()}`);
        // now we need to submit the withdrawal transaction.
        const txHash = await webbRelayer.substrateAnchorWithdraw({
            chain: aliceNode.name,
            id: withdrawalProof.id,
            proof: Array.from(hexToU8a(withdrawalProof.proofBytes)),
            root: Array.from(hexToU8a(withdrawalProof.root)),
            nullifierHash: Array.from(hexToU8a(withdrawalProof.nullifierHash)),
            refund: withdrawalProof.refund,
            fee: withdrawalProof.fee,
            recipient: withdrawalProof.recipient,
            relayer: withdrawalProof.relayer,
            refreshCommitment: withdrawalProof.refreshCommitment,
        });
        expect(txHash).to.be.not.null;

        // get the balance after withdrawal is done and see if it increases
        // @ts-ignore
        const { nonce: nonceAfter, data: balanceAfter } = await api.query.system.account(withdrawalProof.recipient);
        let balanceAfterWithdraw = balanceAfter.free.toBigInt();
        console.log(`balance after withdrawal is ${balanceAfter.free.toBigInt()}`);
        expect(balanceAfterWithdraw > initialBalance);
    });

    after(async () => {
        await aliceNode?.stop();
        await bobNode?.stop();
        await webbRelayer?.stop();
    });
});

// Helper methods, we can move them somewhere if we end up using them again.

async function createAnchorDepositTx(api: ApiPromise): Promise<{
    tx: SubmittableExtrinsic<'promise'>;
    note: Note;
}> {
    const noteInput: NoteGenInput = {
        protocol: 'anchor',
        version: 'v2',
        sourceChain: '2199023256632',
        targetChain: '2199023256632',
        sourceIdentifyingData: '3',
        targetIdentifyingData: '3',
        tokenSymbol: 'WEBB',
        amount: '1',
        denomination: '18',
        backend: 'Arkworks',
        hashFunction: 'Poseidon',
        curve: 'Bn254',
        width: '4',
        exponentiation: '5',
    };
    const note = await Note.generateNote(noteInput);
    const treeId = 4;
    const leaf = note.getLeaf();
    console.log(`leaf deposit ${JSON.stringify(leaf)}`);
    console.log(`leaf deposit ${u8aToHex(leaf)}`);
    //api.tx.anchorBn254!.create!(10000, 2, 30, 0);
    const tx = api.tx.anchorBn254!.deposit!(treeId, leaf);
    console.log(`tx in create anchor deposit ${tx}`);
    return { tx, note };
}

type WithdrawalOpts = {
    relayer: string;
    recipient: string;
    fee?: number;
    refund?: number;
};

type WithdrawalProof = {
    id: number;
    proofBytes: string;
    root: string;
    nullifierHash: string;
    recipient: string;
    relayer: string;
    fee: number;
    refund: number;
    refreshCommitment: string;
};

async function createAnchorWithdrawProof(
    api: ApiPromise,
    note: any,
    opts: WithdrawalOpts
): Promise<WithdrawalProof> {
    try {
        const recipientAddressHex = u8aToHex(decodeAddress(opts.recipient)).replace(
            '0x',
            ''
        );
        const relayerAddressHex = u8aToHex(decodeAddress(opts.relayer)).replace(
            '0x',
            ''
        );
        const treeId = 4;
        //@ts-ignore
        const getLeaves = api.rpc.mt.getLeaves;
        const treeLeaves: Uint8Array[] = await getLeaves(treeId, 0, 511);
       // console.log(`tree leaves are: ${JSON.stringify(treeLeaves)}`);

        // @ts-ignore
        const treeRoot = await api.query.merkleTreeBn254.trees(4);
        //console.log(`tree root is ${treeRoot}`);
        // @ts-ignore
        console.log(`tree root is ${treeRoot.toJSON().root}`);
        // @ts-ignore
        const newTreeRoot = treeRoot.toJSON().root.replace(
            '0x',
            ''
        );

        const pm = new ProvingManagerWrapper('direct-call');
        const leafHex = u8aToHex(note.getLeaf());
        treeLeaves.forEach((l) => console.log(`for each ${u8aToHex(l)}`))
        console.log(`leaf hex withdrawal is: ${leafHex}`);
        const leafIndex = treeLeaves.findIndex((l) => u8aToHex(l) === leafHex);
        console.log(`leaf index is ${leafIndex}`);
        expect(leafIndex).to.be.greaterThan(-1);
        const gitRoot = child
            .execSync('git rev-parse --show-toplevel')
            .toString()
            .trim();
        const provingKeyPath = path.join(
            gitRoot,
            'tests',
            'protocol-substrate-fixtures',
            'fixed-anchor',
            'bn254',
            'x5',
            '2',
            'proving_key_uncompressed.bin'
        );
        const provingKey = fs.readFileSync(provingKeyPath);

        // @ts-ignore
        const proofInput: ProvingManagerSetupInput = {
            note: note.serialize(),
            relayer: relayerAddressHex,
            recipient: recipientAddressHex,
            leaves: treeLeaves,
            leafIndex,
            fee: opts.fee === undefined ? 0 : opts.fee,
            refund: opts.refund === undefined ? 0 : opts.refund,
            provingKey,
            roots: newTreeRoot,
            refreshCommitment: '0000000000000000000000000000000000000000000000000000000000000000'
        };
        //console.log(`proofInput ${JSON.stringify(proofInput)}`);
        const zkProof = await pm.proof(proofInput);
        console.log(`zkProof ${zkProof}`);
        return {
            id: treeId,
            proofBytes: `0x${zkProof.proof}`,
            root: `0x${zkProof.root}`,
            nullifierHash: `0x${zkProof.nullifierHash}`,
            recipient: opts.recipient,
            relayer: opts.relayer,
            fee: opts.fee === undefined ? 0 : opts.fee,
            refund: opts.refund === undefined ? 0 : opts.refund,
            refreshCommitment: '0000000000000000000000000000000000000000000000000000000000000000'
        };
    } catch (error) {
        console.log("error thrown here")
        //@ts-ignore
        console.error(error.error_message);
        //@ts-ignore
        console.error(error.code);
        throw error;
    }
}

function createAccount(accountId: string): any {
    const keyring = new Keyring({ type: 'sr25519' });
    const account = keyring.addFromUri(accountId);

    return account;
}

async function makeDeposit(
    api: any,
    aliceNode: any,
    account: any
): Promise<any> {
    const { tx, note } = await createAnchorDepositTx(api);


    // send the deposit transaction.
    const txSigned = await tx.signAsync(account);

    const executedTrx = await aliceNode.executeTransaction(txSigned);

    return note;
}

async function initWithdrawal(
    api: any,
    webbRelayer: any,
    account: any,
    note: any
): Promise<WithdrawalProof> {
    console.log(`initialize withdrawal`)
    // next we need to prepare the withdrawal transaction.
    // create correct proof with right address
    const withdrawalProof = await createAnchorWithdrawProof(api, note, {
        recipient: account.address,
        relayer: account.address,
    });
    console.log(`withdrawal proof is ${withdrawalProof}`);
    // ping the relayer!
    await webbRelayer.ping();

    return withdrawalProof;
}
