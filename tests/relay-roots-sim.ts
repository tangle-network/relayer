import { LocalChain, LocalChainOpts } from './lib/localTestnet.js';
import { WebbRelayer, EnabledContracts } from './lib/webbRelayer.js';
import { ethers } from 'ethers';
import { DeployerConfig } from '@webb-tools/interfaces';
import { GovernedTokenWrapper } from '@webb-tools/tokens';
import { VBridge } from '@webb-tools/vbridge';
import fs from 'fs';
import path from 'path';
import temp from 'temp';
import getPort, { portNumbers } from 'get-port';
import { CircomUtxo } from '@webb-tools/sdk-core';
import { sleep } from './lib/sleep.js';

export async function fetchComponentsFromFilePaths(wasmPath: string, witnessCalculatorPath: string, zkeyPath: string) {
  const wasm: Buffer = await fs.readFileSync(path.resolve(wasmPath));
  const witnessCalculatorGenerator = await import(witnessCalculatorPath);
  const witnessCalculator = await witnessCalculatorGenerator.default(wasm);
  const zkeyBuffer: Buffer = await fs.readFileSync(path.resolve(zkeyPath));
  const zkey: Uint8Array = new Uint8Array(zkeyBuffer.buffer.slice(zkeyBuffer.byteOffset, zkeyBuffer.byteOffset + zkeyBuffer.byteLength));

  return {
    wasm,
    witnessCalculator,
    zkey
  };
}

async function deploySignatureVBridge(
  tokens: Record<number, string[]>,
  deployers: DeployerConfig
): Promise<VBridge> {
  let assetRecord: Record<number, string[]> = {};
  let chainIdsArray: number[] = [];
  let existingWebbTokens = new Map<number, GovernedTokenWrapper>();
  let governorConfig: Record<number, ethers.Wallet> = {};

  for (const chainIdType of Object.keys(deployers)) {
    assetRecord[chainIdType] = tokens[chainIdType];
    chainIdsArray.push(Number(chainIdType));
    governorConfig[Number(chainIdType)] = deployers[chainIdType];
    existingWebbTokens[chainIdType] = null;
    console.log(tokens[chainIdType]);
  }

  const bridgeInput = {
    vAnchorInputs: {
      asset: assetRecord,
    },
    chainIDs: chainIdsArray,
    webbTokens: existingWebbTokens
  }

  const zkComponentsSmall = await fetchComponentsFromFilePaths(
    path.resolve(
      `./protocol-solidity-fixtures/fixtures/vanchor_2/8/poseidon_vanchor_2_8.wasm`
    ),
    path.resolve(
      `./protocol-solidity-fixtures/fixtures/vanchor_2/8/witness_calculator.cjs`
    ),
    path.resolve(
      `./protocol-solidity-fixtures/fixtures/vanchor_2/8/circuit_final.zkey`
    )
  );
  const zkComponentsLarge = await fetchComponentsFromFilePaths(
    path.resolve(
      `./protocol-solidity-fixtures/fixtures/vanchor_16/8/poseidon_vanchor_16_8.wasm`
    ),
    path.resolve(
      `./protocol-solidity-fixtures/fixtures/vanchor_16/8/witness_calculator.cjs`
    ),
    path.resolve(
      `./protocol-solidity-fixtures/fixtures/vanchor_16/8/circuit_final.zkey`
    )
  );

  return VBridge.deployVariableAnchorBridge(
    bridgeInput,
    deployers,
    governorConfig,
    zkComponentsSmall,
    zkComponentsLarge,
  )
}

async function startChains(): Promise<LocalChain[]> {
  const populatedAccounts = [
    {
      secretKey: '0x0000000000000000000000000000000000000000000000000000000000000001',
      balance: ethers.utils.parseEther('1000').toHexString(),
    },
    {
      secretKey: '0x0000000000000000000000000000000000000000000000000000000000000002',
      balance: ethers.utils.parseEther('1000').toHexString(),
    },
    {
      secretKey: '0x0000000000000000000000000000000000000000000000000000000000000003',
      balance: ethers.utils.parseEther('1000').toHexString(),
    },
  ];

  const enabledContracts: EnabledContracts[] = [
    {
      contract: 'VAnchor',
    },
  ];

  const hermesOpts: LocalChainOpts = {
    name: 'hermes',
    port: 5001,
    chainId: 5001,
    populatedAccounts,
    enabledContracts
  }

  const localHermes = new LocalChain(hermesOpts);

  const athenaOpts: LocalChainOpts = {
    name: 'athena',
    port: 5002,
    chainId: 5002,
    populatedAccounts,
    enabledContracts
  }

  const localAthena = new LocalChain(athenaOpts);

  const demeterOpts: LocalChainOpts = {
    name: 'demeter',
    port: 5003,
    chainId: 5003,
    populatedAccounts,
    enabledContracts
  }

  const localDemeter = new LocalChain(demeterOpts);

  return [localHermes, localAthena, localDemeter];
}

async function runSim() {

  const deployerPK = '0x0000000000000000000000000000000000000000000000000000000000000001';

  const chains = await startChains();
  const [hermesChain, athenaChain, demeterChain] = chains;
  const hermesWallet = new ethers.Wallet(deployerPK, hermesChain!.provider());
  const athenaWallet = new ethers.Wallet(deployerPK, athenaChain!.provider());
  const demeterWallet = new ethers.Wallet(deployerPK, demeterChain!.provider());

  const deployerAddress = hermesWallet.address;
  // const hermesToken = await hermesChain.deployToken('Test token', 'TEST', hermesWallet);
  // const athenaToken = await athenaChain.deployToken('Test token', 'TEST', athenaWallet);
  // const demeterToken = await demeterChain.deployToken('Test token', 'TEST', demeterWallet);

  const deployers: DeployerConfig = {
    [hermesChain!.chainId]: hermesWallet,
    [athenaChain!.chainId]: athenaWallet,
    [demeterChain!.chainId]: demeterWallet,
  };

  const tokens: Record<number, string[]> = {
    [hermesChain!.chainId]: ['0'],
    [athenaChain!.chainId]: ['0'],
    [demeterChain!.chainId]: ['0'],
  };

  const vbridge = await deploySignatureVBridge(tokens, deployers);

  const tmpDirPath = temp.mkdirSync();

  await hermesChain!.writeConfig(`${tmpDirPath}/${hermesChain!.name}.json`, {
    signatureVBridge: vbridge,
    proposalSigningBackend: { type: 'Mocked', privateKey: deployerPK },
  });
  await athenaChain!.writeConfig(`${tmpDirPath}/${athenaChain!.name}.json`, {
    signatureVBridge: vbridge,
    proposalSigningBackend: { type: 'Mocked', privateKey: deployerPK },
  });
  await demeterChain!.writeConfig(`${tmpDirPath}/${demeterChain!.name}.json`, {
    signatureVBridge: vbridge,
    proposalSigningBackend: { type: 'Mocked', privateKey: deployerPK },
  });

  // now start the relayer
  const relayerPort = await getPort({ port: portNumbers(9955, 9999) });
  const webbRelayer = new WebbRelayer({
    port: relayerPort,
    tmp: true,
    configDir: tmpDirPath,
    showLogs: true,
    verbosity: 3,
  });
  await webbRelayer.waitUntilReady();

  // Setup variables for loop execution
  let txCount = 0;
  let failedRootRelay = false;
  let hermesLeaves: Uint8Array[] = [];
  let athenaLeaves: Uint8Array[] = [];
  let demeterLeaves: Uint8Array[] = [];
  let leaves = [hermesLeaves, athenaLeaves, demeterLeaves];

  // loop and create transactions for deposit with the following steps:
  //  - deposit on chain 1,
  //  - deposit on chain 2,
  //  - deposit on chain 3
  while (txCount < 100 && !failedRootRelay) {
    for (let i = 0; i < chains.length; i++) {
      const withdrawAnchorIndex = i > 0 ? i - 1 : chains.length - 1;

      // This index will be the index to prove against on the withdraw
      const valueUtxoIndex = leaves[i]!.length;

      /* deposit */
      const depositAnchor = await vbridge.getVAnchor(chains[i]!.chainId);

      const leavesMapBeforeDeposit: Record<number, Uint8Array[]> = {
        [chains[i]!.chainId]: leaves[i]!,
        [chains[withdrawAnchorIndex]!.chainId]: leaves[withdrawAnchorIndex]!,
      };

      const outputUtxo = await CircomUtxo.generateUtxo({
        backend: 'Circom',
        curve: 'Bn254',
        chainId: chains[withdrawAnchorIndex]!.chainId.toString(),
        originChainId: chains[i]!.chainId.toString(),
        amount: '10000000',
      });

      const dummyOutput1 = await CircomUtxo.generateUtxo({
        backend: 'Circom',
        curve: 'Bn254',
        chainId: chains[i]!.chainId.toString(),
        originChainId: chains[i]!.chainId.toString(),
        amount: '0',
      });

      // Root on deposit anchor before insertions.
      const beforeRoot = await depositAnchor.contract.getLastRoot();
      console.log('beforeRoot: ', beforeRoot);

      // The root that would be on depositAnchor after the deposit occurs
      await depositAnchor.transactWrap(
        '0x0000000000000000000000000000000000000000',
        [],
        [outputUtxo, dummyOutput1],
        0,
        '0x0000000000000000000000000000000000000000',
        '0x0000000000000000000000000000000000000000',
        leavesMapBeforeDeposit
      );

      const latestDepositRoot = await depositAnchor.contract.getLastRoot()
      console.log('root of anchor after deposit: ', latestDepositRoot);
      txCount++;

      leaves[i]!.push(outputUtxo.commitment);
      leaves[i]!.push(dummyOutput1.commitment);

      // Wait for the relayer to relay the roots, allow for 10 seconds to relay the root before
      // assuming the root relay was missed.
      try {
        let currentChain = chains[withdrawAnchorIndex]!;
        const pendingTx = await webbRelayer.waitForEvent({
          kind: 'tx_queue',
          event: {
            ty: 'EVM',
            chain_id: currentChain.underlyingChainId.toString(),
            pending: true,
          },
        });
        const result = await Promise.race([
          // now we wait for the tx queue on that chain to execute the transaction.
          webbRelayer.waitForEvent({
            kind: 'tx_queue',
            event: {
              ty: 'EVM',
              chain_id: currentChain.underlyingChainId.toString(),
              finalized: true,
              tx_hash: pendingTx.tx_hash,
            },
          }),
          sleep(15_000),
        ]);
        await sleep(5_000);
        webbRelayer.clearLogs();
        const withdrawAnchor = await vbridge.getVAnchor(chains[withdrawAnchorIndex]!.chainId);
        const edgeIndex = await withdrawAnchor.contract.edgeIndex(chains[i]!.chainId);
        const edgeList = await withdrawAnchor.contract.edgeList(edgeIndex);
        // If there was an edge that existed, make sure the root was relayed properly
        if (valueUtxoIndex != 0 && edgeList.root !== latestDepositRoot) {
          console.log('edgeList root: ', edgeList.root);
          console.log('latestDepositRoot: ', latestDepositRoot);
          throw new Error('missed root relay!')
        }
      } catch (e) {
        console.log('error relaying root:', e);
        console.log('Successful transaction count: ', txCount);
        failedRootRelay = true;
        break;
      }
    }
  }

  await hermesChain!.stop();
  await athenaChain!.stop();
  await demeterChain!.stop();
  await webbRelayer.stop();
  return;
}

runSim().then(() => process.exit());
