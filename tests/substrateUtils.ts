import { ApiPromise, WsProvider } from '@polkadot/api';
import { KeyringPair } from '@polkadot/keyring/types';
import {
  generate_proof_js,
  JsNote,
  JsNoteBuilder,
  ProofInputBuilder,
} from '@webb-tools/wasm-utils/njs';
import { hexToU8a, u8aToHex } from '@polkadot/util';
import { decodeAddress } from '@polkadot/keyring';
import path from 'path';
import fs from 'fs';
import { generateSubstrateMixerWithdrawRequest } from './relayerUtils';
import { BigNumber } from 'ethers';

export function currencyToUnitI128(currencyAmount: number) {
  let bn = BigNumber.from(currencyAmount);
  return bn.mul(1_000_000_000_000);
}

export async function preparePolkadotApi() {
  const wsProvider = new WsProvider('ws://127.0.0.1:9944');
  const api = await ApiPromise.create({
    provider: wsProvider,
    rpc: {
      mt: {
        getLeaves: {
          description: 'Query for the tree leaves',
          params: [
            {
              name: 'tree_id',
              type: 'u32',
              isOptional: false,
            },
            {
              name: 'from',
              type: 'u32',
              isOptional: false,
            },
            {
              name: 'to',
              type: 'u32',
              isOptional: false,
            },
            {
              name: 'at',
              type: 'Hash',
              isOptional: true,
            },
          ],
          type: 'Vec<[u8; 32]>',
        },
      },
    },
  });
  return api.isReady;
}
type MethodPath = {
  section: string;
  method: string;
};
export function polkadotTx(
  api: ApiPromise,
  path: MethodPath,
  params: any[],
  signer: KeyringPair
) {
  // @ts-ignore
  const tx = api.tx[path.section][path.method](...params);
  return new Promise<string>((resolve, reject) => {
    tx.signAndSend(signer, (result) => {
      const status = result.status;
      const events = result.events.filter(
        ({ event: { section } }) => section === 'system'
      );
      if (status.isInBlock || status.isFinalized) {
        for (const event of events) {
          const {
            event: { data, method },
          } = event;
          const [dispatchError] = data as any;

          if (method === 'ExtrinsicFailed') {
            let message = dispatchError.type;

            if (dispatchError.isModule) {
              try {
                const mod = dispatchError.asModule;
                const error = dispatchError.registry.findMetaError(mod);

                message = `${error.section}.${error.name}`;
              } catch (error) {
                reject(message);
              }
            } else if (dispatchError.isToken) {
              message = `${dispatchError.type}.${dispatchError.asToken.type}`;
            }

            reject(message);
          } else if (method === 'ExtrinsicSuccess') {
            resolve(tx.hash.toString());
          }
        }
      }
    }).catch((e) => reject(e));
  });
}

export async function transferBalance(
  api: ApiPromise,
  source: KeyringPair,
  receiverPairs: KeyringPair[],
  number: number = 1000
) {
  // transfer to alice
  for (const receiverPair of receiverPairs) {

    await polkadotTx(
      api,
      { section: 'balances', method: 'transfer' },
      [receiverPair.address, currencyToUnitI128(number).toString()],
      source
    );
  }
}

export async function setORMLTokenBalance(
  api: ApiPromise,
  sudoPair: KeyringPair,
  receiverPair: KeyringPair,
  ORMLCurrencyId: number = 0,
  amount: number = 1000
) {

  return new Promise((resolve, reject) => {
    const address = api.createType('MultiAddress', {
      Id: receiverPair.address,
    });
    // @ts-ignore
    api.tx.sudo
      .sudo(
        // @ts-ignore
        api.tx.currencies.updateBalance(
          address,
          ORMLCurrencyId,
          currencyToUnitI128(amount)
        )
      )
      .signAndSend(sudoPair, (res) => {
        if (res.isFinalized || res.isCompleted) {
          resolve(null);
        }
        if (res.isError) {
          reject(res.dispatchError);
        }
      });
  });
}

export async function fetchTreeLeaves(
  api: ApiPromise,
  treeId: string | number
): Promise<Uint8Array[]> {
  let done = false;
  let from = 0;
  let to = 511;
  const leaves: Uint8Array[] = [];

  while (done === false) {
    const treeLeaves: any[] = await (api.rpc as any).mt.getLeaves(
      treeId,
      from,
      to
    );
    if (treeLeaves.length === 0) {
      done = true;
      break;
    }
    leaves.push(...treeLeaves.map((i) => i.toU8a()));
    from = to;
    to = to + 511;
  }
  return leaves;
}

export async function depositMixerBnX5_5(
  api: ApiPromise,
  depositor: KeyringPair
) {
  let noteBuilder = new JsNoteBuilder();
  noteBuilder.prefix('webb.mixer');
  noteBuilder.version('v1');

  noteBuilder.sourceChainId('1');
  noteBuilder.targetChainId('1');

  noteBuilder.tokenSymbol('WEBB');
  noteBuilder.amount('1');
  noteBuilder.denomination('18');

  noteBuilder.backend('Arkworks');
  noteBuilder.hashFunction('Poseidon');
  noteBuilder.curve('Bn254');
  noteBuilder.width('5');
  noteBuilder.exponentiation('5');
  const note = noteBuilder.build();
  const leaf = note.getLeafCommitment();

  await polkadotTx(
    api,
    { section: 'mixerBn254', method: 'deposit' },
    [0, leaf],
    depositor
  );
  return note;
}

export async function withdrawMixerBnX5_5(
  api: ApiPromise,
  signer: KeyringPair,
  note: JsNote,
  relayerAccountId: string
) {
  const accountId = signer.address;

  const addressHex = u8aToHex(decodeAddress(accountId));
  const relayerAddressHex = u8aToHex(decodeAddress(relayerAccountId));
  // fetch leaves
  const leaves = await fetchTreeLeaves(api, 0);
  const proofInputBuilder = new ProofInputBuilder();
  const leafHex = u8aToHex(note.getLeafCommitment());
  proofInputBuilder.setNote(note);
  proofInputBuilder.setLeaves(leaves);
  const leafIndex = leaves.findIndex((l) => u8aToHex(l) === leafHex);
  proofInputBuilder.setLeafIndex(String(leafIndex));

  proofInputBuilder.setFee('0');
  proofInputBuilder.setRefund('0');

  proofInputBuilder.setRecipient(addressHex.replace('0x', ''));
  proofInputBuilder.setRelayer(relayerAddressHex.replace('0x', ''));
  const pkPath = path.join(
    // tests path
    process.cwd(),
    '..',
    'protocol-substrate-fixtures',
    'mixer',
    'bn254',
    'x5',
    'proving_key_uncompressed.bin'
  );
  const pk = fs.readFileSync(pkPath);
  proofInputBuilder.setPk(pk.toString('hex'));

  const proofInput = proofInputBuilder.build_js();
  const proofPayload = generate_proof_js(proofInput);

  const req = generateSubstrateMixerWithdrawRequest(
    0,
    0,
    0,

    signer.address,
    relayerAccountId,

    hexToU8a(`0x${proofPayload.nullifierHash}`),
    hexToU8a(`0x${proofPayload.root}`),
    hexToU8a(`0x${proofPayload.proof}`)
  );

  return req;
}
