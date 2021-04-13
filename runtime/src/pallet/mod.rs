use codec::{Decode, Encode};

pub mod merkle;
pub mod mixer;

pub type CurrencyId = u64;
pub type TreeId = u32;
pub type Amount = i128;
pub type BlockLength = u64;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, Encode, Decode)]
pub struct ScalarData(pub [u8; 32]);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, Encode, Decode)]
pub struct Nullifier(pub [u8; 32]);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, Encode, Decode)]
pub struct Commitment(pub [u8; 32]);
