use std::{thread, time::Duration};

use crate::{
    container::{run_scenario, Container},
    ONE_DAY_SECONDS,
};

#[ignore]
#[test]
fn real_world() {
    let now_seconds = time::OffsetDateTime::now_utc().unix_timestamp() as u64;
    let yesterday_timestamp_seconds = now_seconds
        .saturating_sub(ONE_DAY_SECONDS)
        .saturating_div(ONE_DAY_SECONDS)
        .saturating_mul(ONE_DAY_SECONDS);
    let timestamp_seconds = now_seconds / 60 * 60;

    let container = Container::builder().name("real_world").build();

    run_scenario(container, |container| {
        thread::sleep(Duration::from_secs(10));
        Ok(())
    })
    .expect("Scenario failed");
}
