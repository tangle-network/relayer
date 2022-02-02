import fetch from 'node-fetch';
import {
  ChildProcessWithoutNullStreams,
  execSync,
  spawn,
  SpawnOptionsWithoutStdio,
} from 'child_process';

require('dotenv').config({ path: '.env' });

export type RelayerChainConfig = {
  chainName: string;
  contracts: [{ address: string; withdrawFeePercentage: number }];
  beneficiary: string;
};

export const generateTornadoWithdrawRequest = (
  chainName: string,
  contractAddress: string,
  proof: string,
  args: string[]
) => ({
  evm: {
    tornadoRelayTx: {
      chain: chainName,
      contract: contractAddress,
      proof,
      root: args[0],
      nullifierHash: args[1],
      recipient: args[2],
      relayer: args[3],
      fee: args[4],
      refund: args[5],
    },
  },
});

export const generateSubstrateWithdrawRequest = (
  chainName: string,
  treeId: string,
  proof: string,
  args: string[]
) => ({
  sub: {
    mixerRelayTx: {
      chain: chainName,
      id: treeId,
      proof,
      root: args[0],
      nullifierHash: args[1],
      recipient: args[2],
      relayer: args[3],
      fee: args[4],
      refund: args[5],
    },
  },
});

// Convert a hex string to a byte array
function hexStringToByte(str) {
  if (!str) {
    return new Uint8Array();
  }

  var a = [];
  for (var i = 0, len = str.length; i < len; i += 2) {
    // @ts-ignore
    a.push(parseInt(str.substr(i, 2), 16));
  }

  return new Uint8Array(a);
}

export const generateSubstrateMixerWithdrawRequest = (
  fee: number,
  refund: number,
  treeId: number,

  recipient: string,
  relayer: string,

  nullifierHash: Uint8Array,
  root: Uint8Array,
  proof: Uint8Array
) => {
  return {
    substrate: {
      mixerRelayTx: {
        id: treeId,
        chain: 'localnode',
        recipient,
        relayer,

        refund,
        fee,

        nullifierHash: Array.from(nullifierHash),
        root: Array.from(root),
        proof: Array.from(proof),
      },
    },
  };
};
export const generateAnchorWithdrawRequest = (
  chainName: string,
  contractAddress: string,
  proof: string,
  args: string[]
) => {
  let roots = args[0]!.substr(2);
  let rootsBytes = hexStringToByte(roots);
  let rootsArray = Array.from(rootsBytes);

  return {
    evm: {
      anchorRelayTx: {
        chain: chainName,
        contract: contractAddress,
        proof,
        roots: rootsArray,
        refreshCommitment: args[2],
        nullifierHash: args[1],
        recipient: args[3],
        relayer: args[4],
        fee: args[5],
        refund: args[6],
      },
    },
  };
};
export const getRelayerSubstrateConfig = async (
  chainName: string,
  endpoint: string
): Promise<RelayerChainConfig> => {
  const relayerInfoRes = await fetch(`${endpoint}/api/v1/info`);
  const relayerInfo: any = await relayerInfoRes.json();
  console.log(relayerInfo);
  return {
    chainName: chainName,
    beneficiary: relayerInfo.substrate[chainName].beneficiary,
    contracts: relayerInfo.substrate[chainName].contracts,
  };
};

export const getRelayerConfig = async (
  chainName: string,
  endpoint: string
): Promise<RelayerChainConfig> => {
  const relayerInfoRes = await fetch(`${endpoint}/api/v1/info`);
  const relayerInfo: any = await relayerInfoRes.json();

  return {
    chainName: chainName,
    beneficiary: relayerInfo.evm[chainName].beneficiary,
    contracts: relayerInfo.evm[chainName].contracts,
  };
};
function spawnWithLogger(command: string, options?: SpawnOptionsWithoutStdio) {
  const process = spawn(command, options);
  process.stdout.on('data', (data) => {
    console.log(data.toString());
  });

  process.stderr.on('data', (data) => {
    console.error(data.toString());
  });

  process.on('close', (code) => {
    console.log(` process ${process.pid} exited with code ${code}`);
  });

  return process;
}
export function startDarkWebbNode(): ChildProcessWithoutNullStreams {
  execSync(
    'docker pull ghcr.io/webb-tools/protocol-substrate-standalone-node:edge',
    { stdio: 'inherit' }
  );
  const node1 =
    'darkwebb-standalone-node --dev --alice --node-key 0000000000000000000000000000000000000000000000000000000000000001';
  const node2 =
    'darkwebb-standalone-node --dev --bob --port 33334 --tmp   --bootnodes /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp';

  const getDockerCmd = (cmd: string, ports: number[]) => {
    return `docker run --rm  ${ports.reduce(
      (acc, port) => `${acc} -p ${port}:${port}`,
      ''
    )} ghcr.io/webb-tools/protocol-substrate-standalone-node:edge  ${cmd}`;
  };

  const proc = spawnWithLogger(
    `${getDockerCmd(node1, [9944, 30333])} & ${getDockerCmd(
      node2,
      [33334, 33334]
    )}`,
    {
      shell: true,
    }
  );

  return proc;
}

export function startWebbRelayer(
  portNumber: number = 9955
): [ChildProcessWithoutNullStreams, string] {
  const proc = spawn(
    `../target/debug/webb-relayer`,
    [
      '-vvv',
      '-c',
      './config',
      '--tmp', // use tmp dir for the database
    ],
    {
      env: {
        ...process.env,
        WEBB_PORT: `${portNumber}`,
      },
    }
  );
  proc.stdout.on('data', (data) => {
    console.log(data.toString());
  });

  proc.stderr.on('data', (data) => {
    console.error(data.toString());
  });

  proc.on('close', (code) => {
    console.log(`relayer process exited with code ${code}`);
  });

  return [proc, `http://localhost:${portNumber}`];
}

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export enum Result {
  Continue,
  CleanExit,
  Errored,
}

export function handleMessage(data: any): Result {
  if (data.error || data.withdraw?.errored) {
    return Result.Errored;
  } else if (data.network === 'invalidRelayerAddress') {
    return Result.Errored;
  } else if (data.withdraw?.finalized) {
    return Result.CleanExit;
  } else if (data.withdraw?.droppedFromMemPool) {
    return Result.Errored;
  } else {
    return Result.Continue;
  }
}
