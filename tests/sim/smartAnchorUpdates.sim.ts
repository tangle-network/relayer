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

/*
 * Smart Anchor Updates Simulation
 * Summary:
 *
 * This test simulates the following scenario:
 * 1. Busy Bridge Traffic:
 *  a. User A deposits X ETH N times on Chain A targeting Chain B
 * Print the following metrics:
 * 1. Total Number of Transactions sent by the relayer.
 * 2. Total Gas Used by the relayer.
 *
 * 2. Busy Bridge Traffic with Smart Anchor Updates:
 *  a. User A deposits X ETH N times on Chain A targeting Chain B
 * Print the following metrics:
 * 1. Total Number of Transactions sent by the relayer.
 * 2. Total Gas Used by the relayer.
 *
 * At the end of the simulation, print the following metrics:
 * 1. Improvement of Number of Transactions sent by the relayer From Scenario 1 to Scenario 2
 * 2. Improvement of Total Gas Used by the relayer From Scenario 1 to Scenario 2
 */
import Chai, { expect } from 'chai';
import ChaiAsPromised from 'chai-as-promised';
import { VBridge } from '@webb-tools/protocol-solidity';
import { TokenConfig } from '@webb-tools/vbridge/dist/VBridge';
import { CircomUtxo, Keypair } from '@webb-tools/sdk-core';
import dotenv from 'dotenv';
import { BigNumber, ethers } from 'ethers';
import temp from 'temp';
import { LocalChain } from '../lib/localTestnet.js';
import {
  defaultRelayerFeeConfigValue,
  EnabledContracts,
  LinkedAnchor,
  WebbRelayer,
} from '../lib/webbRelayer.js';
import { hexToU8a, u8aToHex } from '@polkadot/util';

dotenv.config({ path: '../.env' });
Chai.use(ChaiAsPromised);

type SimulationMetrics = {
  // Total Gas Used by the relayer across all chains.
  totalGasUsed: string;
  // Average Gas Used by the relayer for each chain.
  avgGasUsed: string;
  // Total Number of Transactions sent by the relayer.
  // across all chains.
  totalRelayerTxCount: number;
  // Average Number of Transactions sent by the relayer.
  // for each chain.
  avgRelayerTxCount: number;
  // Total Number of milliseconds it took to relay all roots.
  // across all chains.
  totalWaitTimeToRelayAllRoots: number;
};

describe.skip('Smart Anchor Updates Simulation', function () {
  // Disable mocha time-out because these tests take a long time.
  this.timeout(0);
  this.slow(Number.MAX_SAFE_INTEGER);

  const NUM_CHAINS = 7;
  const NUM_DEPOSITS = 50;

  const simulationMetrics: SimulationMetrics[] = [];
  let chains: LocalChain[];
  let signatureVBridge: VBridge.VBridge;
  let senderWallet: ethers.Wallet;
  let relayerWallet: ethers.Wallet;

  let webbRelayer: WebbRelayer;

  this.beforeEach(async () => {
    const tmpDirPath = temp.mkdirSync();
    const govPk = u8aToHex(ethers.utils.randomBytes(32));
    const relayerPk = u8aToHex(ethers.utils.randomBytes(32));
    relayerWallet = new ethers.Wallet(relayerPk);

    senderWallet = ethers.Wallet.createRandom();
    const deployerWallet = ethers.Wallet.createRandom();

    const enabledContracts: EnabledContracts[] = [
      {
        contract: 'VAnchor',
      },
    ];
    const populatedAccounts = [
      {
        secretKey: relayerPk,
        balance: ethers.utils.parseEther('10').toHexString(),
      },
      {
        secretKey: senderWallet.privateKey,
        balance: ethers.utils.parseEther('10').toHexString(),
      },
      {
        secretKey: deployerWallet.privateKey,
        balance: ethers.utils.parseEther('1').toHexString(),
      },
    ];

    chains = await Promise.all(
      Array(NUM_CHAINS)
        .fill(0)
        .map((_, i) =>
          LocalChain.init({
            port: 5000 + i,
            chainId: 5000 + i,
            name: `chain${i}`,
            populatedAccounts,
            enabledContracts,
            evmOptions: {
              miner: {
                defaultGasPrice: ethers.utils
                  .parseUnits('75', 'gwei')
                  .toHexString(),
              },
            },
          })
        )
    );

    const wrappedTokens: TokenConfig[] = [];
    const unwrappedTokens: string[] = [];

    for (const chain of chains) {
      const wrappedToken = await chain.deployToken('Wrapped Ethereum', 'WETH');
      wrappedTokens.push(wrappedToken);
      unwrappedTokens.push('0x0000000000000000000000000000000000000000');
    }

    const deployerWallets = chains.map((chain) => {
      return deployerWallet.connect(chain.provider());
    });
    const gov = new ethers.Wallet(govPk);
    const govConfig = chains.reduce((acc, chain) => {
      acc[chain.chainId] = gov.address;
      return acc;
    }, {} as Record<number, string>);

    signatureVBridge = await LocalChain.deployManySignatureVBridge(
      chains,
      wrappedTokens,
      unwrappedTokens,
      deployerWallets,
      govConfig
    );

    const linkedAnchors: LinkedAnchor[] = [];
    for (let i = 0; i < chains.length; i++) {
      const vanchor = signatureVBridge.getVAnchor(chains[i]!.chainId);
      const rid = await vanchor.createResourceId();
      linkedAnchors.push({ type: 'Raw', resourceId: rid });
    }

    for (const chain of chains) {
      const vanchor = signatureVBridge.getVAnchor(chain.chainId);
      const rid = await vanchor.createResourceId();
      const myLinkedAnchors = linkedAnchors.filter((linkedAnchor) => {
        if (linkedAnchor.type === 'Raw') {
          return linkedAnchor.resourceId !== rid;
        } else {
          throw new Error('Not implemented');
        }
      });
      const path = `${tmpDirPath}/${chain.name}.json`;
      await chain.writeConfig(path, {
        signatureVBridge,
        relayerFeeConfig: defaultRelayerFeeConfigValue,
        relayerWallet: relayerWallet,
        linkedAnchors: myLinkedAnchors,
        smartAnchorUpdates: {
          enabled: simulationMetrics.length > 0,
          minTimeDelay: 10,
          maxTimeDelay: 90,
          initialTimeDelay: 10,
        },
        proposalSigningBackend: {
          type: 'Mocked',
          privateKey: govPk,
        },
      });
    }

    for (const chain of chains) {
      const gov = new ethers.Wallet(govPk);
      // Sanity check.
      const governorAddress = gov.address;
      const currentGovernor = await signatureVBridge
        .getVBridgeSide(chain.chainId)
        .contract.governor();
      expect(currentGovernor).to.eq(governorAddress);
    }

    webbRelayer = new WebbRelayer({
      commonConfig: {
        features: { dataQuery: true, governanceRelay: true },
        port: 9955,
      },
      tmp: true,
      configDir: tmpDirPath,
      showLogs: false,
      verbosity: 3,
    });
    await webbRelayer.waitUntilReady();
  });

  it('Busy Bridge Traffic', async () => {
    const localChain1 = chains[0]!;
    const localChain2 = chains[1]!;
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    const senderWallet1 = senderWallet.connect(localChain1.provider());
    vanchor1.setSigner(senderWallet1);

    const tokenAddress = '0x0000000000000000000000000000000000000000';

    const depositUtxos = await Promise.all(
      Array(NUM_DEPOSITS)
        .fill(0)
        .map(() =>
          CircomUtxo.generateUtxo({
            curve: 'Bn254',
            backend: 'Circom',
            amount: ethers.utils.parseEther('0.01').toHexString(),
            originChainId: localChain1.chainId.toString(),
            chainId: localChain2.chainId.toString(),
            keypair: new Keypair(),
          })
        )
    );
    for (const utxo of depositUtxos) {
      const leaves = vanchor1.tree
        .elements()
        .map((el) => hexToU8a(el.toHexString()));

      await vanchor1.transact([], [utxo], 0, 0, '0', '0', tokenAddress, {
        [localChain1.chainId]: leaves,
      });
    }
    const p1 = performance.now();
    await allRootsGotRelayed(signatureVBridge, chains);
    const p2 = performance.now();
    const diff = p2 - p1;
    const metrics = await collectMetrics(chains, relayerWallet);
    metrics.totalWaitTimeToRelayAllRoots = diff;
    simulationMetrics.push(metrics);
  });

  it('Busy Bridge Traffic With Smart Anchor Updates', async () => {
    const localChain1 = chains[0]!;
    const localChain2 = chains[1]!;
    const vanchor1 = signatureVBridge.getVAnchor(localChain1.chainId);
    const senderWallet1 = senderWallet.connect(localChain1.provider());
    vanchor1.setSigner(senderWallet1);

    const tokenAddress = '0x0000000000000000000000000000000000000000';

    const depositUtxos = await Promise.all(
      Array(NUM_DEPOSITS)
        .fill(0)
        .map(() =>
          CircomUtxo.generateUtxo({
            curve: 'Bn254',
            backend: 'Circom',
            amount: ethers.utils.parseEther('0.01').toHexString(),
            originChainId: localChain1.chainId.toString(),
            chainId: localChain2.chainId.toString(),
            keypair: new Keypair(),
          })
        )
    );
    for (const utxo of depositUtxos) {
      const leaves = vanchor1.tree
        .elements()
        .map((el) => hexToU8a(el.toHexString()));

      await vanchor1.transact([], [utxo], 0, 0, '0', '0', tokenAddress, {
        [localChain1.chainId]: leaves,
      });
    }

    const p1 = performance.now();
    await allRootsGotRelayed(signatureVBridge, chains);
    const p2 = performance.now();
    const diff = p2 - p1;
    const metrics = await collectMetrics(chains, relayerWallet);
    metrics.totalWaitTimeToRelayAllRoots = diff;
    simulationMetrics.push(metrics);
  });

  this.afterEach(async () => {
    await webbRelayer?.stop();
    for (const chain of chains) {
      await chain.stop();
    }
  });

  this.afterAll(async () => {
    console.log(`Simulation Metrics for ${NUM_CHAINS} chains`);
    console.log(`For ${NUM_DEPOSITS} deposits:`);
    console.table(simulationMetrics);
    console.log('Diff (across all chains):');
    const v1 = simulationMetrics[0]!;
    const v2 = simulationMetrics[1]!;

    const totalGasSaved = ethers.utils
      .parseEther(v1.totalGasUsed)
      .sub(ethers.utils.parseEther(v2.totalGasUsed));

    const totalGasSavedPercentStr = ethers.utils.formatEther(totalGasSaved);
    console.log(`Total gas saved: ${totalGasSavedPercentStr}`);
    const totalTxSaved = v1.totalRelayerTxCount - v2.totalRelayerTxCount;
    console.log(`Total txs saved: ${totalTxSaved}`);
    const totalTxSavedPercent = (totalTxSaved / v1.totalRelayerTxCount) * 100;
    console.log(`Total txs saved percent: ${totalTxSavedPercent}%`);
  });
});

async function allRootsGotRelayed(
  signatureVBridge: VBridge.VBridge,
  chains: LocalChain[]
) {
  const allKnown: boolean[] = chains.map(() => false);
  while (true) {
    const allIsKnown = allKnown.every((v) => v);
    if (allIsKnown) break;

    for (let i = 0; i < chains.length; i++) {
      if (allKnown[i]) continue;
      const chainA = chains[i]!;
      const vanchorA = signatureVBridge.getVAnchor(chainA.chainId);
      const expectedRoot = await vanchorA.contract.getLastRoot();
      const chainAPassed: boolean[] = Array(chains.length).fill(false);
      for (let j = 0; j < chains.length; j++) {
        if (i === j) {
          chainAPassed[j] = true;
          continue;
        }
        const chainB = chains[j]!;
        const vanchorB = signatureVBridge.getVAnchor(chainB.chainId);

        const neigborRoots = await vanchorB.contract.getLatestNeighborRoots();
        const isKnownNeighborRoot = neigborRoots.some(
          (root: BigNumber) => root.toHexString() === expectedRoot.toHexString()
        );
        chainAPassed[j] = isKnownNeighborRoot;
      }
      allKnown[i] = chainAPassed.every((x) => x === true);
    }
  }
}
async function collectMetrics(
  chains: LocalChain[],
  relayerWallet: ethers.Wallet
): Promise<SimulationMetrics> {
  let totalTransactions = 0;
  let mTotalGasUsed = BigNumber.from(0);
  for (const chain of chains) {
    const wallet = relayerWallet.connect(chain.provider());
    const nonce = await wallet.getTransactionCount();
    totalTransactions += nonce;
    const balanceAtStart = await wallet.getBalance(0);
    const balanceNow = await wallet.getBalance();
    const gasUsed = balanceAtStart.sub(balanceNow);
    mTotalGasUsed = mTotalGasUsed.add(gasUsed);
  }
  const totalRelayerTxCount = totalTransactions;
  const avgRelayerTxCount = Math.round(totalTransactions / chains.length);
  const totalGasUsed = ethers.utils.formatEther(mTotalGasUsed);
  const avgGasUsed = ethers.utils.formatEther(mTotalGasUsed.div(chains.length));
  return {
    totalRelayerTxCount,
    avgRelayerTxCount,
    totalGasUsed,
    avgGasUsed,
    totalWaitTimeToRelayAllRoots: 0,
  };
}
