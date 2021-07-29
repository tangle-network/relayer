use webb::evm::ethereum_types::{U256};
use std::ops::Mul;
use std::ops::Div;

pub mod utils {
    pub fn calculate_bigInt_fee(feePercent: f32, principle: U256) -> U256 {
        let millFee = (feePercent * 1000000.0) as u32;
        let millU256: U256 = principle.mul(millFee);
        let feeU256: U256 = millU256.div(1000000);
        return feeU256
    }
}

mod test {

    #[test]
    fn percentFee() {
        let submitted_value: U256 = U256::from_dec_str("5000000000000000").ok().unwrap();
        let expected_fee: U256 = U256::from_dec_str("250000000000000").ok().unwrap();
        let withdraw_fee_percent_dec: f32 = 0.05;
        let formatted_fee: U256 = calculateBigIntFee(withdraw_fee_percent_dec, submitted_value);

        assert_eq!(expected_fee, formatted_fee);
    }
}
