use ic_cdk::api::call::{msg_cycles_accept, msg_cycles_available};

use crate::{candid::ExchangeRateError, utils, XRC_REQUEST_CYCLES_COST};

pub(crate) enum ChargeCyclesError {
    NotEnoughCycles,
    FailedToAcceptCycles,
}

impl From<ChargeCyclesError> for ExchangeRateError {
    fn from(error: ChargeCyclesError) -> Self {
        match error {
            ChargeCyclesError::NotEnoughCycles => ExchangeRateError::NotEnoughCycles,
            ChargeCyclesError::FailedToAcceptCycles => ExchangeRateError::FailedToAcceptCycles,
        }
    }
}

pub(crate) trait Environment {
    /// Gets the current IC time in seconds.
    fn time_secs(&self) -> u64 {
        utils::time_secs()
    }

    /// Gets the cycles that have been sent in the current message.
    fn cycles_available(&self) -> u64 {
        msg_cycles_available()
    }

    /// Accepts the cycles up to a given max amount from the current message.
    fn accept_cycles(&self, max_amount: u64) -> u64 {
        msg_cycles_accept(max_amount)
    }

    /// Checks if enough cycles have been sent defined by [XRC_REQUEST_CYCLES_COST].
    /// If there are enough cycles, accept the cycles up to the [XRC_REQUEST_CYCLES_COST].
    fn charge_cycles(&self) -> Result<(), ChargeCyclesError> {
        if self.cycles_available() < XRC_REQUEST_CYCLES_COST {
            return Err(ChargeCyclesError::NotEnoughCycles);
        }

        let accepted = self.accept_cycles(XRC_REQUEST_CYCLES_COST);
        if accepted != XRC_REQUEST_CYCLES_COST {
            return Err(ChargeCyclesError::FailedToAcceptCycles);
        }

        Ok(())
    }
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
    use super::Environment;

    /// An environment that simulates pieces of the canister API in order to exercise
    /// the canister's endpoints.
    #[derive(Default)]
    pub(crate) struct TestEnvironment {
        cycles_available: u64,
        cycles_accepted: u64,
        time_secs: u64,
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
        fn cycles_available(&self) -> u64 {
            self.cycles_available
        }

        fn accept_cycles(&self, _: u64) -> u64 {
            self.cycles_accepted
        }

        fn time_secs(&self) -> u64 {
            self.time_secs
        }
    }
}
