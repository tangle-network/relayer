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
import fs from 'fs';
import ganache from 'ganache';
import { ethers } from 'ethers';
import { Server } from 'ganache';
import { Bridges, Interfaces } from '@webb-tools/protocol-solidity';
import {
  BridgeInput,
  DeployerConfig,
  GovernorConfig,
} from '@webb-tools/interfaces';
import { MintableToken } from '@webb-tools/tokens';
import { fetchComponentsFromFilePaths } from '@webb-tools/utils';
import path from 'path';
import child from 'child_process';
import {
  ChainInfo,
  Contract,
  EventsWatcher,
  SigningBackend,
} from './webbRelayer';
import { ConvertToKebabCase } from './tsHacks';

export type GanacheAccounts = {
  balance: string;
  secretKey: string;
};

export function startGanacheServer(
  port: number,
  networkId: number,
  populatedAccounts: GanacheAccounts[],
  options: any = {}
): Server<'ethereum'> {
  const ganacheServer = ganache.server({
    accounts: populatedAccounts,
    quiet: true,
    network_id: networkId,
    chainId: networkId,
    ...options,
  });

  ganacheServer.listen(port).then(() => {
    process.stdout.write(`Ganache Started on http://127.0.0.1:${port} ..\n`);
  });

  return ganacheServer;
}

type LocalChainOpts = {
  name: string;
  port: number;
  chainId: number;
  populatedAccounts: GanacheAccounts[];
};

export class LocalChain {
  public readonly endpoint: string;
  private readonly server: Server<'ethereum'>;
  private signatureBridge: Bridges.SignatureBridge | null = null;
  constructor(public readonly opts: LocalChainOpts) {
    this.endpoint = `http://127.0.0.1:${opts.port}`;
    this.server = startGanacheServer(
      opts.port,
      opts.chainId,
      opts.populatedAccounts
    );
  }

  public get name(): string {
    return this.opts.name;
  }

  public get chainId(): number {
    return this.opts.chainId;
  }

  public provider(): ethers.providers.WebSocketProvider {
    return new ethers.providers.WebSocketProvider(this.endpoint);
  }

  public async stop() {
    await this.server.close();
  }

  public async deployToken(
    name: string,
    symbol: string,
    wallet: ethers.Wallet
  ): Promise<MintableToken> {
    return MintableToken.createToken(name, symbol, wallet);
  }

  public async deploySignatureBridge(
    otherChain: LocalChain,
    localToken: MintableToken,
    otherToken: MintableToken,
    localWallet: ethers.Wallet,
    otherWallet: ethers.Wallet
  ): Promise<Bridges.SignatureBridge> {
    const gitRoot = child
      .execSync('git rev-parse --show-toplevel')
      .toString()
      .trim();
    localWallet.connect(this.provider());
    otherWallet.connect(otherChain.provider());
    const bridgeInput: BridgeInput = {
      anchorInputs: {
        asset: {
          [this.opts.chainId]: [localToken.contract.address],
          [otherChain.opts.chainId]: [otherToken.contract.address],
        },
        anchorSizes: [ethers.utils.parseEther('1')],
      },
      chainIDs: [this.opts.chainId, otherChain.opts.chainId],
    };
    const deployerConfig: DeployerConfig = {
      [this.opts.chainId]: localWallet,
      [otherChain.opts.chainId]: otherWallet,
    };
    const initialGovernors: GovernorConfig = {
      [this.opts.chainId]: localWallet,
      [otherChain.opts.chainId]: otherWallet,
    };
    // copy the witness_calculator.js file to @webb-tools/utils, but use the .cjs extension
    // to avoid the babel compiler to compile it.
    const witnessCalculatorPath = path.join(
      gitRoot,
      'tests',
      'protocol-solidity-fixtures/fixtures/bridge/2/witness_calculator.js'
    );
    const witnessCalculatorCjsPath = path.join(
      gitRoot,
      'tests',
      'node_modules/@webb-tools/utils/witness_calculator.cjs'
    );
    // check if the cjs file exists, if not, copy the js file to the cjs file
    if (!fs.existsSync(witnessCalculatorCjsPath)) {
      fs.copyFileSync(witnessCalculatorPath, witnessCalculatorCjsPath);
    }
    const zkComponents = await fetchComponentsFromFilePaths(
      path.join(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/bridge/2/poseidon_bridge_2.wasm'
      ),
      witnessCalculatorCjsPath,
      path.join(
        gitRoot,
        'tests',
        'protocol-solidity-fixtures/fixtures/bridge/2/circuit_final.zkey'
      )
    );

    const val = await Bridges.SignatureBridge.deployFixedDepositBridge(
      bridgeInput,
      deployerConfig,
      initialGovernors,
      zkComponents
    );
    this.signatureBridge = val;
    return val;
  }

  public async exportConfig(
    signatureBridge?: Bridges.SignatureBridge,
    signingBackend?: SigningBackend
  ): Promise<FullChainInfo> {
    const bridge = signatureBridge ?? this.signatureBridge;
    if (!bridge) {
      throw new Error('Signature bridge not deployed yet');
    }
    const localAnchor = bridge.getAnchor(
      this.opts.chainId,
      ethers.utils.parseEther('1')
    );
    const side = bridge.getBridgeSide(this.opts.chainId);
    const wallet = side.governor;
    const otherChainIds = Array.from(bridge.bridgeSides.keys()).filter(
      (chainId) => chainId !== this.opts.chainId
    );
    const otherAnchors = otherChainIds.map(
      (chainId) =>
        [chainId, bridge.getAnchor(chainId, ethers.utils.parseEther('1'))] as [
          number,
          Interfaces.IAnchor
        ]
    );

    const chainInfo: FullChainInfo = {
      enabled: true,
      httpEndpoint: this.endpoint,
      wsEndpoint: this.endpoint.replace('http', 'ws'),
      chainId: this.opts.chainId,
      beneficiary: wallet.address,
      privateKey: wallet.privateKey,
      contracts: [
        // first the local Anchor
        {
          contract: 'Anchor',
          address: localAnchor.getAddress(),
          deployedAt: 1,
          size: 1, // Ethers
          withdrawFeePercentage: 0,
          'dkg-node': signingBackend ?? undefined,
          eventsWatcher: { enabled: true, pollingInterval: 1000 },
          linkedAnchors: otherAnchors.map(([chainId, anchor]) => ({
            chain: chainId.toString(),
            address: anchor.getAddress(),
          })),
        },
        {
          contract: 'SignatureBridge',
          address: side.contract.address,
          deployedAt: 1,
          eventsWatcher: { enabled: true, pollingInterval: 1000 },
        },
      ],
    };
    return chainInfo;
  }

  public async writeConfig(
    path: string,
    signatureBridge?: Bridges.SignatureBridge,
    signingBackend?: SigningBackend
  ): Promise<void> {
    const config = await this.exportConfig(signatureBridge, signingBackend);
    // don't mind my typescript typing here XD
    type ConvertedContract = Omit<
      ConvertToKebabCase<Contract>,
      'events-watcher'
    > & {
      'events-watcher': ConvertToKebabCase<EventsWatcher>;
    };
    type ConvertedConfig = Omit<
      ConvertToKebabCase<typeof config>,
      'contracts'
    > & {
      contracts: ConvertedContract[];
    };
    type FullConfigFile = {
      evm: {
        // chainId as the chain identifier
        [key: number]: ConvertedConfig;
      };
    };

    const convertedConfig: ConvertedConfig = {
      enabled: config.enabled,
      'http-endpoint': config.httpEndpoint,
      'ws-endpoint': config.wsEndpoint,
      'chain-id': config.chainId,
      beneficiary: config.beneficiary,
      'private-key': config.privateKey,
      contracts: config.contracts.map((contract) => ({
        contract: contract.contract,
        address: contract.address,
        'deployed-at': contract.deployedAt,
        size: contract.size,
        'dkg-node': contract['dkg-node'],
        'withdraw-gaslimit': '0x5B8D80',
        'withdraw-fee-percentage': contract.withdrawFeePercentage,
        'events-watcher': {
          enabled: contract.eventsWatcher.enabled,
          'polling-interval': contract.eventsWatcher.pollingInterval,
        },
        'linked-anchors': contract.linkedAnchors,
      })),
    };
    const fullConfigFile: FullConfigFile = {
      evm: {
        [this.opts.chainId]: convertedConfig,
      },
    };
    const configString = JSON.stringify(fullConfigFile, null, 2);
    fs.writeFileSync(path, configString);
  }
}

export type FullChainInfo = ChainInfo & {
  httpEndpoint: string;
  wsEndpoint: string;
  privateKey: string;
};
