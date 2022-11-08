use crate::{
    rate_limiting, types::HttpResponse, with_cache, with_forex_rate_store, AllocatedBytes,
    MetricCounter,
};
use ic_cdk::api::time;
use serde_bytes::ByteBuf;
use std::{fmt::Display, io};

pub fn get_metrics() -> HttpResponse {
    let now = time();
    let mut writer = MetricsEncoder::new(vec![], now / 1_000_000);
    match encode_metrics(&mut writer) {
        Ok(()) => {
            let body = writer.into_inner();
            HttpResponse {
                status_code: 200,
                headers: vec![
                    (
                        "Content-Type".to_string(),
                        "text/plain; version=0.0.4".to_string(),
                    ),
                    ("Content-Length".to_string(), body.len().to_string()),
                ],
                body: ByteBuf::from(body),
            }
        }
        Err(err) => HttpResponse {
            status_code: 500,
            headers: vec![],
            body: ByteBuf::from(format!("Failed to encode metrics: {}", err)),
        },
    }
}

fn encode_metrics(w: &mut MetricsEncoder<Vec<u8>>) -> std::io::Result<()> {
    w.encode_counter(
        "xrc_requests",
        MetricCounter::GetExchangeRateRequest.get() as u64,
        "The number of requests the canister has received.",
    )?;

    w.encode_counter(
        "xrc_cmc_requests",
        MetricCounter::GetExchangeRateRequestFromCmc.get() as u64,
        "The number of requests from the CMC canister.",
    )?;

    w.encode_counter(
        "xrc_cycles_related_errors",
        MetricCounter::CycleRelatedErrors.get() as u64,
        "The number of errors that have resulted in a cycle-related issue.",
    )?;

    w.encode_counter(
        "xrc_errors_returned",
        MetricCounter::ErrorsReturned.get() as u64,
        "The number of errors that have been returned.",
    )?;

    w.encode_counter(
        "xrc_cmc_errors_returned",
        MetricCounter::ErrorsReturnedToCmcCounter.get() as u64,
        "The number of errors that have been returned to the CMC canister.",
    )?;

    w.encode_gauge(
        "xrc_http_outcall_requests",
        rate_limiting::get_request_counter() as f64,
        "The current number of HTTP outcalls.",
    )?;

    with_forex_rate_store(|store| {
        w.encode_gauge(
            "xrc_forex_store_size_bytes",
            store.allocated_bytes() as f64,
            "The current size of the forex rate store in bytes.",
        )
    })?;

    with_cache(|cache| {
        w.encode_gauge(
            "xrc_cache_size",
            cache.size() as f64,
            "The current size of the exchange rate cache.",
        )?;
        Ok(())
    })
}

// `MetricsEncoder` provides methods to encode metrics in a text format
// that can be understood by Prometheus.
//
// Metrics are encoded with the block time included, to allow Prometheus
// to discard out-of-order samples collected from replicas that are behind.
//
// See [Exposition Formats][1] for an informal specification of the text format.
//
// [1]: https://github.com/prometheus/docs/blob/master/content/docs/instrumenting/exposition_formats.md
struct MetricsEncoder<W: io::Write> {
    writer: W,
    now_millis: u64,
}

impl<W: io::Write> MetricsEncoder<W> {
    /// Constructs a new encoder dumping metrics with the given timestamp into
    /// the specified writer.
    fn new(writer: W, now_millis: u64) -> Self {
        Self { writer, now_millis }
    }

    /// Returns the internal buffer that was used to record the
    /// metrics.
    fn into_inner(self) -> W {
        self.writer
    }

    fn encode_header(&mut self, name: &str, help: &str, typ: &str) -> io::Result<()> {
        writeln!(self.writer, "# HELP {} {}", name, help)?;
        writeln!(self.writer, "# TYPE {} {}", name, typ)
    }

    fn encode_single_value<T: Display>(
        &mut self,
        typ: &str,
        name: &str,
        value: T,
        help: &str,
    ) -> io::Result<()> {
        self.encode_header(name, help, typ)?;
        writeln!(self.writer, "{} {} {}", name, value, self.now_millis)
    }

    /// Encodes the metadata and the value of a gauge.
    fn encode_gauge(&mut self, name: &str, value: f64, help: &str) -> io::Result<()> {
        self.encode_single_value("gauge", name, value, help)
    }

    #[allow(dead_code)]
    fn encode_counter(&mut self, name: &str, value: u64, help: &str) -> io::Result<()> {
        self.encode_single_value("counter", name, value, help)
    }
}
