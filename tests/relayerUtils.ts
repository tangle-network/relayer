import fetch from 'node-fetch';
import { spawn } from 'child_process';
require('dotenv').config({ path: '.env' });

export type RelayerChainConfig = {
  chainName: string;
  withdrawFeePercentage: number;
  withdrawGaslimit: string;
  account: string;
};

export const generateWithdrawRequest = (
  chainName: string,
  contractAddress: string,
  proof: string,
  args: string[]
) => ({
  evm: {
    [chainName]: {
      relayWithdraw: {
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
  },
});

export const getRelayerConfig = async (
  chainName: string
): Promise<RelayerChainConfig> => {
  const relayerInfoRes = await fetch(process.env.RELAYER_ENDPOINT_HTTP || 'http://localhost:9955/api/v1/info');
  const relayerInfo: any = await relayerInfoRes.json();

  return {
    chainName: chainName,
    account: relayerInfo.evm[chainName].account,
    withdrawFeePercentage: relayerInfo.evm[chainName].withdrawFeePercentage,
    withdrawGaslimit: relayerInfo.evm[chainName].withdrawGaslimit,
  };
};

export async function startWebbRelayer() {
  const proc = spawn('../target/debug/webb-relayer', [
    '-vvv',
    '-c',
    './config.toml',
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
  } else {
    return Result.Continue;
  }
}
