import { u8aToHex, hexToU8a, assert } from '@polkadot/util';
import {
  decodeProposalHeader,
  encodeProposalHeader,
  ProposalHeader,
} from './webbProposals.js';

const LE = true;
const BE = false;
export const enum ChainIdType {
  UNKNOWN = 0x0000,
  EVM = 0x0100,
  SUBSTRATE = 0x0200,
  POLKADOT_RELAYCHAIN = 0x0301,
  KUSAMA_RELAYCHAIN = 0x0302,
  COSMOS = 0x0400,
  SOLANA = 0x0500,
}

export interface ResourceIdUpdateProposal {
  /**
   * The ResourceIdUpdateProposal Header.
   * This is the first 40 bytes of the proposal.
   * See `encodeProposalHeader` for more details.
   */
  readonly header: ProposalHeader;
  /**
   * 32 bytes Hex-encoded string.
   */
  readonly newResourceId: string;
  /**
   * 1 byte Hex-encoded string.
   */
  readonly palletIndex: string;
  /**
   * 1 byte Hex-encoded string.
   */
  readonly callIndex: string;
}
export function encodeResourceIdUpdateProposal(
  proposal: ResourceIdUpdateProposal
): Uint8Array {
  const header = encodeProposalHeader(proposal.header);
  const resourceIdUpdateProposal = new Uint8Array(74);
  resourceIdUpdateProposal.set(header, 0); // 0 -> 40
  const palletIndex = hexToU8a(proposal.palletIndex).slice(0, 1);
  resourceIdUpdateProposal.set(palletIndex, 40); // 40 -> 41
  const callIndex = hexToU8a(proposal.callIndex).slice(0, 1);
  resourceIdUpdateProposal.set(callIndex, 41); // 40 -> 41
  const newResourceId = hexToU8a(proposal.newResourceId).slice(0, 32);
  resourceIdUpdateProposal.set(newResourceId, 42); // 42 -> 74
  return resourceIdUpdateProposal;
}

export function decodeResourceIdUpdateProposal(
  data: Uint8Array
): ResourceIdUpdateProposal {
  const header = decodeProposalHeader(data.slice(0, 40)); // 0 -> 40
  const palletIndex = u8aToHex(data.slice(40, 41)); // 40 -> 41
  const callIndex = u8aToHex(data.slice(41, 42)); // 41 -> 42
  const newResourceId = u8aToHex(data.slice(42, 74)); // 42 -> 74

  return {
    header,
    newResourceId,
    palletIndex,
    callIndex,
  };
}
