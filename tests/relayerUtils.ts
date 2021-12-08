import fetch from 'node-fetch';
import { spawn } from 'child_process';
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

// Convert a hex string to a byte array
function hexStringToByte(str) {
  if (!str) {
    return new Uint8Array();
  }

  var a = [];
  for (var i = 0, len = str.length; i < len; i+=2) {
    // @ts-ignore
    a.push(parseInt(str.substr(i,2),16));
  }

  return new Uint8Array(a);
}

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

export const getRelayerConfig = async (
  chainName: string,
  endpoint: string
): Promise<RelayerChainConfig> => {
  const relayerInfoRes = await fetch(`${endpoint}/api/v1/info`);
  console.log(relayerInfoRes);
  const relayerInfo: any = await relayerInfoRes.json();

  return {
    chainName: chainName,
    beneficiary: relayerInfo.evm[chainName].beneficiary,
    contracts: relayerInfo.evm[chainName].contracts,
  };
};

export function startWebbRelayer() {
  const proc = spawn('../target/debug/webb-relayer', [
    '-vvv',
    '-c',
    './config',
  ]);
  proc.stdout.on('data', (data) => {
    console.log(data.toString());
  });

  proc.stderr.on('data', (data) => {
    console.error(data.toString());
  });

  proc.on('close', (code) => {
    console.log(`relayer process exited with code ${code}`);
  });

  return proc;
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
