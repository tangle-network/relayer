import fetch from 'node-fetch';
import { spawn } from 'child_process';

export type RelayerChainConfig = {
  withdrewFeePercentage: number;
  withdrewGaslimit: string;
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
      relayWithdrew: {
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
  const relayerInfoRes = await fetch('http://localhost:9955/api/v1/info');
  const relayerInfo: any = await relayerInfoRes.json();

  return {
    account: relayerInfo.evm[chainName].account,
    withdrewFeePercentage: relayerInfo.evm[chainName].withdrewFeePercentage,
    withdrewGaslimit: relayerInfo.evm[chainName].withdrewGaslimit,
  };
};

export async function startWebbRelayer() {
  const proc = spawn('../target/debug/webb-relayer', [
    '-vvvv',
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
  } else if (data.withdraw?.finlized) {
    return Result.CleanExit;
  } else {
    return Result.Continue;
  }
}
