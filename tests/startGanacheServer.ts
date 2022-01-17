import ganache from 'ganache';

export type GanacheAccounts = {
  balance: string;
  secretKey: string;
};

export async function startGanacheServer(
  port: number,
  networkId: number,
  populatedAccounts: GanacheAccounts[],
  options: any = {}
) {
  const ganacheServer = ganache.server({
    accounts: populatedAccounts,
    blockTime: 1,
    quiet: true,
    network_id: networkId,
    chainId: networkId,
    ...options,
  });

  await ganacheServer.listen(port);
  console.log(`Ganache Started on http://127.0.0.1:${port} ..`);

  return ganacheServer;
}
