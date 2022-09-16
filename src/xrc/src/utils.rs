const NANOS_PER_SEC: u64 = 1_000_000_000;

pub fn now_seconds() -> u64 {
    let now = ic_cdk::api::time();
    now / NANOS_PER_SEC
}
