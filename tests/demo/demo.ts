import { LocalChain } from '../lib/localTestnet.js';
import { LocalDkg } from '../lib/localDkg.js';
import { defaultEventsWatcherValue, UsageMode } from '../lib/substrateNodeBase.js';
import path from 'path';
import { EnabledContracts, Pallet } from '../lib/webbRelayer.js';
import getPort, { portNumbers } from 'get-port';
import { ethers } from 'ethers';
import { ethAddressFromUncompressedPublicKey } from '../lib/ethHelperFunctions.js';
import { timeout } from '../lib/timeout.js';
import inquirer from 'inquirer';
import { Tokens, VBridge } from '@webb-tools/protocol-solidity';

function printConfig(vbridge: VBridge.VBridge, chains: LocalChain[]){
  for (let chain of chains) {
    const bridgeSide = vbridge.getVBridgeSide(chain.chainId);
    const vanchor = vbridge.getVAnchor(chain.chainId);

    console.log(`VAnchor for chain ${chain.name}: ${vanchor.contract.address}`);
    console.log(`BridgeSide for chain ${chain.name}: ${bridgeSide.contract.address}`);
  }
}

async function run () {

  /* setup constants */
  const configDirPath = path.resolve('demo/config');
  const usageMode: UsageMode = {
    mode: 'host',
    nodePath: path.resolve('../../dkg-substrate/target/release/dkg-standalone-node'),
  }

  const enabledPallets: Pallet[] = [
    {
      pallet: 'DKGProposalHandler',
      eventsWatcher: defaultEventsWatcherValue,
    },
    {
      pallet: 'DKG',
      eventsWatcher: defaultEventsWatcherValue,
    },
  ];
  const enabledContracts: EnabledContracts[] = [
    {
      contract: 'VAnchor',
    },
  ];

  const deployerPrivateKey = '0x0000000000000000000000000000000000000000000000000000000000000001';
  const relayerPrivateKey = '0x0000000000000000000000000000000000000000000000000000000000000002';

  const populatedAccounts = [
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: deployerPrivateKey,
    },
    {
      balance: ethers.utils.parseEther('1000').toHexString(),
      secretKey: relayerPrivateKey,
    },
  ]

  /* Deploy the DKG nodes and write the config for the DKG node */
  const aliceDkgNode = await LocalDkg.start({
    name: 'aliceDkg',
    authority: 'alice',
    usageMode,
    ports: 'auto',
    enabledPallets
  });
  const bobDkgNode = await LocalDkg.start({
    name: 'bobDkg',
    authority: 'bob',
    usageMode,
    ports: 'auto',
    enabledPallets
  });
  const charlieDkgNode = await LocalDkg.start({
    name: 'charlieDkg',
    authority: 'charlie',
    usageMode,
    ports: 'auto',
    enabledPallets
  });

  let runningNodes = [aliceDkgNode, bobDkgNode, charlieDkgNode];

  // After starting nodes, wrap all code in a try block to catch any error and terminate process
  try {

    // Only need to startup the relayer on one DKG node,
    // choose to write the config for charlie
    let chainId = await charlieDkgNode.getChainId();
    await charlieDkgNode.writeConfig(`${configDirPath}/${charlieDkgNode.name}.json`, {
      suri: '//Charlie',
      chainId: chainId
    });

    /* Wait for the DKG to startup, and public key emitted. */
    await charlieDkgNode.waitForEvent({
      section: 'dkg',
      method: 'PublicKeySubmitted',
    });

    /* Start the chains */
    const hermesPort = await getPort({ port: portNumbers(3333,4444) });
    const hermesChain = new LocalChain({
      port: hermesPort,
      chainId: hermesPort,
      name: 'Hermes',
      populatedAccounts,
      enabledContracts,
      ganache: {
        miner: {
          blockTime: 1,
        }
      }
    });
    const hermesDeployerWallet = new ethers.Wallet(deployerPrivateKey, hermesChain.provider());
    const hermesRelayerWallet = new ethers.Wallet(relayerPrivateKey, hermesChain.provider());

    const athenaPort = await getPort({ port: portNumbers(3333,4444) });
    const athenaChain = new LocalChain({
      port: athenaPort,
      chainId: athenaPort,
      name: 'Athena',
      populatedAccounts,
      enabledContracts,
      ganache: {
        miner: {
          blockTime: 1,
        }
      }
    });
    const athenaDeployerWallet = new ethers.Wallet(deployerPrivateKey, athenaChain.provider());
    const athenaRelayerWallet = new ethers.Wallet(relayerPrivateKey, athenaChain.provider());

    const demeterPort = await getPort({ port: portNumbers(3333,4444) });
    const demeterChain = new LocalChain({
      port: demeterPort,
      chainId: demeterPort,
      name: 'Demeter',
      populatedAccounts,
      enabledContracts,
      ganache: {
        miner: {
          blockTime: 1,
        }
      }
    });
    const demeterDeployerWallet = new ethers.Wallet(deployerPrivateKey, demeterChain.provider());
    const demeterRelayerWallet = new ethers.Wallet(relayerPrivateKey, demeterChain.provider());

    /* deploy EVM contracts and write the config */
    const hermesWETH = await hermesChain.deployToken('WETH', 'WETH', hermesDeployerWallet);
    const athenaWETH = await athenaChain.deployToken('WETH', 'WETH', athenaDeployerWallet);
    const demeterWETH = await athenaChain.deployToken('WETH', 'WETH', demeterDeployerWallet);

    const signatureVBridge = await LocalChain.deployVBridge(
      [hermesChain, athenaChain, demeterChain],
      [hermesWETH, athenaWETH, demeterWETH],
      [hermesDeployerWallet, athenaDeployerWallet, demeterDeployerWallet]
    );
    await hermesChain.writeConfig(`${configDirPath}/${hermesChain.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: {
        type: 'DKGNode',
        node: charlieDkgNode.name,
      }
    });
    await athenaChain.writeConfig(`${configDirPath}/${athenaChain.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: {
        type: 'DKGNode',
        node: charlieDkgNode.name,
      }
    });
    await demeterChain.writeConfig(`${configDirPath}/${demeterChain.name}.json`, {
      signatureVBridge,
      proposalSigningBackend: {
        type: 'DKGNode',
        node: charlieDkgNode.name,
      }
    });

    /* Set permissions for the anchor */
    const hermesAnchor = signatureVBridge.getVAnchor(hermesChain.chainId);
    const hermesWebbTokenAddress = signatureVBridge.getWebbTokenAddress(hermesChain.chainId);
    let token = await Tokens.MintableToken.tokenFromAddress(hermesWebbTokenAddress!, hermesDeployerWallet);
    let tx = await token.approveSpending(hermesAnchor.contract.address);
    await tx.wait();
    await token.mintTokens(hermesRelayerWallet.address, ethers.utils.parseEther('10'));

    const athenaAnchor = signatureVBridge.getVAnchor(athenaChain.chainId);
    const athenaWebbTokenAddress = signatureVBridge.getWebbTokenAddress(athenaChain.chainId);
    token = await Tokens.MintableToken.tokenFromAddress(athenaWebbTokenAddress!, athenaDeployerWallet);
    tx = await token.approveSpending(athenaAnchor.contract.address);
    await tx.wait();
    await token.mintTokens(athenaRelayerWallet.address, ethers.utils.parseEther('10'));

    const demeterAnchor = signatureVBridge.getVAnchor(demeterChain.chainId);
    const demeterWebbTokenAddress = signatureVBridge.getWebbTokenAddress(demeterChain.chainId);
    token = await Tokens.MintableToken.tokenFromAddress(demeterWebbTokenAddress!, demeterDeployerWallet);
    tx = await token.approveSpending(demeterAnchor.contract.address);
    await tx.wait();
    await token.mintTokens(demeterRelayerWallet.address, ethers.utils.parseEther('10'));

    const api = await charlieDkgNode.api();
    const resourceId1 = await hermesAnchor.createResourceId();
    const resourceId2 = await athenaAnchor.createResourceId();
    const resourceId3 = await athenaAnchor.createResourceId();

    const call = (resourceId: string) =>
      api.tx.dkgProposals!.setResource!(resourceId, '0x00');
    // register the resource on DKG node.
    for (const rid of [resourceId1, resourceId2, resourceId3]) {
      await charlieDkgNode.sudoExecuteTransaction(call(rid));
    }

    /* Transfer ownership of the bridge to the address derived from public key */
    const dkgPublicKey = await charlieDkgNode.fetchDkgPublicKey();
    const governorAddress = ethAddressFromUncompressedPublicKey(dkgPublicKey!);
    
    const hermesBridgeSide = signatureVBridge.getVBridgeSide(hermesChain.chainId);
    tx = await hermesBridgeSide.transferOwnership(governorAddress, 1);
    await tx.wait();

    const athenaBridgeSide = signatureVBridge.getVBridgeSide(athenaChain.chainId);
    tx = await athenaBridgeSide.transferOwnership(governorAddress, 1);
    await tx.wait();

    const demeterBridgeSide = signatureVBridge.getVBridgeSide(demeterChain.chainId);
    tx = await demeterBridgeSide.transferOwnership(governorAddress, 1);
    await tx.wait();

    /* forceIncrease the nonce on the dkg. This is done because deployment implementation requires
      a transferOwnership call to the dkg. 
    */
    const forceIncrementNonce = await api.tx.dkg!.manualIncrementNonce!();
    await timeout(
      charlieDkgNode.sudoExecuteTransaction(forceIncrementNonce),
      30_000
    );

    /* startup the relayer --- Manually */
    

    /* CLI for doing actions which create tokenAdd proposals, etc. */
    printConfig(signatureVBridge, [hermesChain, athenaChain, demeterChain]);
    const options = ['print config', 'exit', 'add token'];

    const questions = [
      {
        type: 'list',
        name: 'action',
        choices: options,
        message: 'Choose an action'
      },
      {
        type: 'input',
        name: 'tokenAddress',
        message: 'Enter the token address:',
        when(answers) {
          return answers.action === 'add token'
        }
      }
    ];

    let running = true;

    while(running) {
      const answers = await inquirer.prompt(questions);

      if (answers.action === 'print config') {
        printConfig(signatureVBridge, [hermesChain, athenaChain, demeterChain]);
      } else if (answers.action === 'add token') {
        console.log('tried to add token: ', answers.tokenAddress);
      } else if (answers.action === 'exit') {
        running = false;
      }
    }
  } catch (e) {
    for (const node of runningNodes) {
      await node.stop();
    }
    throw e;
  }
}

run().catch(async (e) => {
  console.log(e.message);
});
