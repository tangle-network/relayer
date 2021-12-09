import ganache from 'ganache-cli';

export type GanacheAccounts = {
  balance: string;
  secretKey: string;
};

export function startGanacheServer(
  port: number,
  networkId: number,
  populatedAccounts: GanacheAccounts[],
  options: any = {}
) {
  const ganacheServer = ganache.server({
    accounts: populatedAccounts,
    port: port,
    network_id: networkId,
    _chainId: networkId,
    chainId: networkId,
    _chainIdRpc: networkId,
    ...options,
  });

  ganacheServer.listen(port);
  console.log(`Ganache Started on http://127.0.0.1:${port} ..`);

  return ganacheServer;
}
