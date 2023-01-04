use ic_cdk::{
    api::call::{msg_cycles_accept, msg_cycles_available},
    caller,
    export::Principal,
};

use crate::{
    candid::ExchangeRateError, utils, XRC_IMMEDIATE_REFUND_CYCLES,
    XRC_OUTBOUND_HTTP_CALL_CYCLES_COST, XRC_RATE_LIMITED_COST, XRC_REQUEST_CYCLES_COST,
};

pub(crate) enum ChargeCyclesError {
    NotEnoughCycles,
}

impl From<ChargeCyclesError> for ExchangeRateError {
    fn from(error: ChargeCyclesError) -> Self {
        match error {
            ChargeCyclesError::NotEnoughCycles => ExchangeRateError::NotEnoughCycles,
        }
    }
}

pub(crate) trait Environment {
    /// Gets the current caller.
    fn caller(&self) -> Principal {
        caller()
    }

    /// Gets the current IC time in seconds.
    fn time_secs(&self) -> u64 {
        utils::time_secs()
    }

    /// Gets the cycles that have been sent in the current message.
    fn cycles_available(&self) -> u64 {
        msg_cycles_available()
    }

    /// Accepts the cycles up to a given maximum amount from the current message.
    fn accept_cycles(&self, max_amount: u64) -> u64 {
        msg_cycles_accept(max_amount)
    }

    /// Checks if the call has enough cycles attached.
    fn has_enough_cycles(&self) -> bool {
        self.cycles_available() >= XRC_REQUEST_CYCLES_COST
    }

    /// Checks if enough cycles have been sent as defined by [XRC_REQUEST_CYCLES_COST].
    /// If there are enough cycles, accept the cycles up to the [XRC_REQUEST_CYCLES_COST].
    fn charge_cycles(
        &self,
        outbound_rates_needed: usize,
        rate_limited: bool,
    ) -> Result<(), ChargeCyclesError> {
        if !self.has_enough_cycles() {
            return Err(ChargeCyclesError::NotEnoughCycles);
        }

        let fee = calculate_fee(outbound_rates_needed, rate_limited);
        let accepted = self.accept_cycles(fee);
        if accepted != fee {
            // We should panic here as this will cause a refund of the cycles to occur.
            panic!("Failed to accept cycles");
        }

        Ok(())
    }
}

/// This function calculates the fee based off the number of outbound requests needed in order
/// to calculate the rate.
fn calculate_fee(outbound_rates_needed: usize, rate_limited: bool) -> u64 {
    if rate_limited {
        return XRC_RATE_LIMITED_COST;
    }

    let fee = match outbound_rates_needed {
        // No requests are needed.
        0 => {
            let unused_cycles = XRC_OUTBOUND_HTTP_CALL_CYCLES_COST
                .checked_mul(2)
                .expect("Unable to double the outbound HTTP cycles cost");
            XRC_REQUEST_CYCLES_COST.checked_sub(unused_cycles).expect(
                "Cannot subtract the unused cycles from the base cost as it causes an underflow",
            )
        }
        // Only 1 request is needed.
        1 => XRC_REQUEST_CYCLES_COST
            .checked_sub(XRC_OUTBOUND_HTTP_CALL_CYCLES_COST)
            .expect(
                "Cannot subtract the unused cycles from the base cost as it causes an underflow",
            ),
        // 2 or more (stablecoin) requests are needed.
        _ => XRC_REQUEST_CYCLES_COST,
    };
    fee.checked_sub(XRC_IMMEDIATE_REFUND_CYCLES)
        .expect("Cannot subtract the refund from fee as it causes an underflow")
}
/// An environment that interacts with the canister API.
pub(crate) struct CanisterEnvironment;

impl CanisterEnvironment {
    /// Construct a new [CanisterEnvironment].
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl Environment for CanisterEnvironment {}

#[cfg(test)]
pub mod test {
    use super::*;

    /// An environment that simulates pieces of the canister API in order to exercise
    /// the canister's endpoints.
    pub(crate) struct TestEnvironment {
        caller: Principal,
        cycles_available: u64,
        cycles_accepted: u64,
        time_secs: u64,
    }

    impl Default for TestEnvironment {
        fn default() -> Self {
            Self {
                caller: Principal::from_text("rrkah-fqaaa-aaaaa-aaaaq-cai")
                    .expect("Failed to create test principal"),
                cycles_available: Default::default(),
                cycles_accepted: Default::default(),
                time_secs: Default::default(),
            }
        }
    }

    impl TestEnvironment {
        /// Returns a new [TestEnvironmentBuilder].
        pub(crate) fn builder() -> TestEnvironmentBuilder {
            TestEnvironmentBuilder::new()
        }
    }

    /// A builder for creating new [TestEnvironment]s.
    pub(crate) struct TestEnvironmentBuilder {
        env: TestEnvironment,
    }

    impl TestEnvironmentBuilder {
        /// Instantiates a new [TestEnvironmentBuilder].
        pub(crate) fn new() -> Self {
            Self {
                env: TestEnvironment::default(),
            }
        }

        /// Sets the [TestEnviroment]'s `caller` field.
        pub(crate) fn with_caller(mut self, caller: Principal) -> Self {
            self.env.caller = caller;
            self
        }

        /// Sets the [TestEnviroment]'s `cycles_available` field.
        pub(crate) fn with_cycles_available(mut self, cycles_available: u64) -> Self {
            self.env.cycles_available = cycles_available;
            self
        }

        /// Sets the [TestEnviroment]'s `cycles_accepted` field.
        pub(crate) fn with_accepted_cycles(mut self, cycles_accepted: u64) -> Self {
            self.env.cycles_accepted = cycles_accepted;
            self
        }

        /// Sets the [TestEnviroment]'s `time_secs` field.
        #[allow(dead_code)]
        pub(crate) fn with_time_secs(mut self, time_secs: u64) -> Self {
            self.env.time_secs = time_secs;
            self
        }

        /// Returns the built TestEnvironment.
        pub(crate) fn build(self) -> TestEnvironment {
            self.env
        }
    }

    impl Environment for TestEnvironment {
        fn caller(&self) -> Principal {
            self.caller
        }

        fn cycles_available(&self) -> u64 {
            self.cycles_available
        }

        fn accept_cycles(&self, cycles_accepted: u64) -> u64 {
            // Exit early if `self.cycles_accepted` is 0
            // Used so we can mimic being unable to accept cycles.
            if self.cycles_accepted == 0 {
                return self.cycles_accepted;
            }

            assert_eq!(
                cycles_accepted, self.cycles_accepted,
                "Cycles accepted ({}) should be equal to what is set in the environment ({}).",
                cycles_accepted, self.cycles_accepted
            );
            self.cycles_accepted
        }

        fn time_secs(&self) -> u64 {
            self.time_secs
        }
    }
}
