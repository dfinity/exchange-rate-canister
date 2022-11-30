pub(crate) trait Environment {
    fn trap(&self, message: &str) -> !;
    fn time(&self) -> u64;
}

pub(crate) struct CanisterEnvironment;

impl Environment for CanisterEnvironment {
    fn trap(&self, message: &str) -> ! {
        ic_cdk::trap(message);
    }

    fn time(&self) -> u64 {
        ic_cdk::api::time()
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

    #[derive(Default)]
    pub(crate) struct TestEnvironment {
        time: u64,
    }

    impl TestEnvironment {
        /// Returns a new [TestEnvironmentBuilder].
        pub(crate) fn builder() -> TestEnvironmentBuilder {
            TestEnvironmentBuilder::new()
        }
    }

    impl Environment for TestEnvironment {
        fn trap(&self, message: &str) -> ! {
            panic!("{}", message);
        }

        fn time(&self) -> u64 {
            self.time
        }
    }

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

        /// Sets the [TestEnviroment]'s `time_secs` field.
        #[allow(dead_code)]
        pub(crate) fn with_time(mut self, time: u64) -> Self {
            self.env.time = time;
            self
        }

        /// Returns the built TestEnvironment.
        pub(crate) fn build(self) -> TestEnvironment {
            self.env
        }
    }
}
