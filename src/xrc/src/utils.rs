use crate::candid::GetExchangeRateRequest;
use ic_cdk::export::Principal;

/// Id of the cycles minting canister on the IC (rkp4c-7iaaa-aaaaa-aaaca-cai).
const MAINNET_CYCLES_MINTING_CANISTER_ID: Principal =
    Principal::from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01]);

const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Gets the current time in seconds.
pub fn time_secs() -> u64 {
    let now = ic_cdk::api::time();
    now / NANOS_PER_SEC
}

/// The function returns the median of the provided values.
pub fn get_median(values: &mut [u64]) -> u64 {
    values.sort();

    let length = values.len();
    if length % 2 == 0 {
        (values[(length / 2) - 1] + values[length / 2]) / 2
    } else {
        values[length / 2]
    }
}

pub fn get_normalized_timestamp(request: &GetExchangeRateRequest) -> u64 {
    (request.timestamp.unwrap_or_else(time_secs) / 60) * 60
}

pub fn is_caller_the_cmc(caller: &Principal) -> bool {
    *caller == MAINNET_CYCLES_MINTING_CANISTER_ID
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn cycles_minting_canister_id_is_correct() {
        let principal_from_text = Principal::from_text("rkp4c-7iaaa-aaaaa-aaaca-cai")
            .expect("should be a valid textual principal ID");
        assert_eq!(MAINNET_CYCLES_MINTING_CANISTER_ID, principal_from_text);
    }
}
