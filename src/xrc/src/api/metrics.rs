use crate::{
    metric_names, rate_limiting, types::HttpResponse, with_cache, with_forex_rate_store,
    with_labeled_counters, with_labeled_gauges, AllocatedBytes, MetricCounter,
};
use ic_cdk::api::time;
use serde_bytes::ByteBuf;
use std::{collections::HashSet, fmt::Display, io};

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
        "The number of requests from the cycles minting canister.",
    )?;

    w.encode_counter(
        "xrc_cycles_related_errors",
        MetricCounter::CycleRelatedErrors.get() as u64,
        "The number of cycle-related errors that have been returned.",
    )?;

    w.encode_counter(
        "xrc_errors_returned",
        MetricCounter::ErrorsReturned.get() as u64,
        "The number of errors that have been returned.",
    )?;

    w.encode_counter(
        "xrc_cmc_errors_returned",
        MetricCounter::ErrorsReturnedToCmc.get() as u64,
        "The number of errors that have been returned to the cycles minting canister.",
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
            cache.len() as f64,
            "The current size of the exchange rate cache.",
        )
    })?;

    w.encode_counter(
        "xrc_pending_errors",
        MetricCounter::PendingErrorsReturned.get() as u64,
        "The number of pending errors returned.",
    )?;

    w.encode_counter(
        "xrc_rate_limited_errors",
        MetricCounter::RateLimitedErrors.get() as u64,
        "The number of rate limited errors returned.",
    )?;

    w.encode_counter(
        "xrc_stablecoin_errors",
        MetricCounter::StablecoinErrorsReturned.get() as u64,
        "The number of stablecoin errors returned.",
    )?;

    w.encode_counter(
        "xrc_crypto_asset_errors",
        MetricCounter::CryptoAssetRelatedErrorsReturned.get() as u64,
        "The number of crypto asset related errors returned.",
    )?;

    w.encode_counter(
        "xrc_forex_asset_errors",
        MetricCounter::ForexAssetRelatedErrorsReturned.get() as u64,
        "The number of forex asset related errors returned.",
    )?;

    w.encode_counter(
        "xrc_inconsistent_rate_errors",
        MetricCounter::InconsistentRatesErrorsReturned.get() as u64,
        "The number of inconsistent rate errors returned.",
    )?;

    w.encode_gauge(
        "ledger_stable_memory_pages",
        ic_cdk::stable::stable_size() as f64,
        "Size of the stable memory allocated by this canister measured in 64K Wasm pages.",
    )?;
    w.encode_gauge(
        "stable_memory_bytes",
        (ic_cdk::stable::stable_size() * 64 * 1024) as f64,
        "Size of the stable memory allocated by this canister measured in bytes.",
    )?;
    w.encode_gauge(
        "heap_memory_bytes",
        heap_memory_size_bytes() as f64,
        "Size of the heap memory allocated by this canister measured in bytes.",
    )?;

    encode_labeled_counter_family(
        w,
        metric_names::FOREX_FETCH_TOTAL,
        "Per-forex source fetch outcomes, labeled by forex source name and outcome.",
    )?;
    encode_labeled_gauge_family(
        w,
        metric_names::FOREX_LAST_SUCCESS_SECONDS,
        "Unix timestamp (seconds) of the most recent successful fetch per forex source.",
    )?;
    encode_labeled_gauge_family(
        w,
        metric_names::PERIODIC_FOREX_RUN_LAST_SECONDS,
        "Unix timestamp (seconds) of the most recent periodic forex-update task run (heartbeat — not contingent on rate-fetch success).",
    )?;

    Ok(())
}

/// Emits one line per labeled series for the given counter metric `name`.
/// `BTreeMap` iteration is already sorted by `(metric_name, labels)`, so
/// the scrape output is naturally ordered without any per-call collect+sort.
fn encode_labeled_counter_family(
    w: &mut MetricsEncoder<Vec<u8>>,
    name: &'static str,
    help: &str,
) -> io::Result<()> {
    with_labeled_counters(|m| -> io::Result<()> {
        for ((_, labels), value) in m.iter().filter(|((n, _), _)| *n == name) {
            let refs: Vec<(&str, &str)> = labels.iter().map(|(k, v)| (*k, v.as_str())).collect();
            w.encode_counter_with_labels(name, &refs, *value, help)?;
        }
        Ok(())
    })
}

/// Emits one line per labeled series for the given gauge metric `name`.
/// Sorted iteration comes from `BTreeMap`; see `encode_labeled_counter_family`.
fn encode_labeled_gauge_family(
    w: &mut MetricsEncoder<Vec<u8>>,
    name: &'static str,
    help: &str,
) -> io::Result<()> {
    with_labeled_gauges(|m| -> io::Result<()> {
        for ((_, labels), value) in m.iter().filter(|((n, _), _)| *n == name) {
            let refs: Vec<(&str, &str)> = labels.iter().map(|(k, v)| (*k, v.as_str())).collect();
            w.encode_gauge_with_labels(name, &refs, *value, help)?;
        }
        Ok(())
    })
}

/// Returns the amount of heap memory in bytes that has been allocated.
#[cfg(target_arch = "wasm32")]
pub fn heap_memory_size_bytes() -> usize {
    const WASM_PAGE_SIZE_BYTES: usize = 65536;
    core::arch::wasm32::memory_size(0) * WASM_PAGE_SIZE_BYTES
}

#[cfg(not(any(target_arch = "wasm32")))]
pub fn heap_memory_size_bytes() -> usize {
    0
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
    /// Names for which the labeled path has already emitted `# HELP`/`# TYPE`.
    /// Only the labeled methods consult this set, so the unlabeled methods
    /// keep their historical "always emit a header" contract — useful as a
    /// guardrail against a future caller that accidentally registers two
    /// unrelated metrics under the same name.
    labeled_headers_seen: HashSet<String>,
}

impl<W: io::Write> MetricsEncoder<W> {
    /// Constructs a new encoder dumping metrics with the given timestamp into
    /// the specified writer.
    fn new(writer: W, now_millis: u64) -> Self {
        Self {
            writer,
            now_millis,
            labeled_headers_seen: HashSet::new(),
        }
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

    /// Emits the header only on the first call for a given `name` *via the
    /// labeled path*. Multiple labeled series sharing one metric name
    /// therefore share one `# HELP`/`# TYPE` pair.
    fn encode_header_once_for_labeled(
        &mut self,
        name: &str,
        help: &str,
        typ: &str,
    ) -> io::Result<()> {
        if !self.labeled_headers_seen.insert(name.to_string()) {
            return Ok(());
        }
        self.encode_header(name, help, typ)
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

    fn encode_labeled_value<T: Display>(
        &mut self,
        typ: &str,
        name: &str,
        labels: &[(&str, &str)],
        value: T,
        help: &str,
    ) -> io::Result<()> {
        self.encode_header_once_for_labeled(name, help, typ)?;
        if labels.is_empty() {
            return writeln!(self.writer, "{} {} {}", name, value, self.now_millis);
        }
        write!(self.writer, "{}{{", name)?;
        for (i, (k, v)) in labels.iter().enumerate() {
            if i > 0 {
                write!(self.writer, ",")?;
            }
            write!(self.writer, "{}=\"", k)?;
            write_escaped_label_value(&mut self.writer, v)?;
            write!(self.writer, "\"")?;
        }
        writeln!(self.writer, "}} {} {}", value, self.now_millis)
    }

    /// Encodes the metadata and the value of a gauge.
    fn encode_gauge(&mut self, name: &str, value: f64, help: &str) -> io::Result<()> {
        self.encode_single_value("gauge", name, value, help)
    }

    fn encode_counter(&mut self, name: &str, value: u64, help: &str) -> io::Result<()> {
        self.encode_single_value("counter", name, value, help)
    }

    fn encode_gauge_with_labels(
        &mut self,
        name: &str,
        labels: &[(&str, &str)],
        value: f64,
        help: &str,
    ) -> io::Result<()> {
        self.encode_labeled_value("gauge", name, labels, value, help)
    }

    fn encode_counter_with_labels(
        &mut self,
        name: &str,
        labels: &[(&str, &str)],
        value: u64,
        help: &str,
    ) -> io::Result<()> {
        self.encode_labeled_value("counter", name, labels, value, help)
    }
}

fn write_escaped_label_value<W: io::Write>(writer: &mut W, value: &str) -> io::Result<()> {
    for c in value.chars() {
        match c {
            '\\' => write!(writer, "\\\\")?,
            '\n' => write!(writer, "\\n")?,
            '"' => write!(writer, "\\\"")?,
            _ => write!(writer, "{}", c)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    fn encode(now_millis: u64, f: impl FnOnce(&mut MetricsEncoder<Vec<u8>>)) -> String {
        let mut encoder = MetricsEncoder::new(vec![], now_millis);
        f(&mut encoder);
        String::from_utf8(encoder.into_inner()).expect("encoder must emit utf-8")
    }

    #[test]
    fn label_less_counter_format_is_unchanged() {
        let out = encode(7, |e| {
            e.encode_counter("xrc_requests", 42, "Total requests.").unwrap();
        });
        assert_eq!(
            out,
            "# HELP xrc_requests Total requests.\n# TYPE xrc_requests counter\nxrc_requests 42 7\n"
        );
    }

    #[test]
    fn label_less_gauge_format_is_unchanged() {
        let out = encode(7, |e| {
            e.encode_gauge("xrc_cache_size", 17.0, "Cache size.").unwrap();
        });
        assert_eq!(
            out,
            "# HELP xrc_cache_size Cache size.\n# TYPE xrc_cache_size gauge\nxrc_cache_size 17 7\n"
        );
    }

    #[test]
    fn labeled_counter_emits_braces_and_pairs() {
        let out = encode(123, |e| {
            e.encode_counter_with_labels(
                "xrc_exchange_fetch_total",
                &[
                    ("exchange", "Mexc"),
                    ("kind", "crypto"),
                    ("outcome", "success"),
                ],
                5,
                "Per-exchange fetch outcomes.",
            )
            .unwrap();
        });
        assert_eq!(
            out,
            concat!(
                "# HELP xrc_exchange_fetch_total Per-exchange fetch outcomes.\n",
                "# TYPE xrc_exchange_fetch_total counter\n",
                "xrc_exchange_fetch_total{exchange=\"Mexc\",kind=\"crypto\",outcome=\"success\"} 5 123\n",
            )
        );
    }

    #[test]
    fn multiple_labeled_series_share_one_header() {
        let out = encode(0, |e| {
            e.encode_counter_with_labels(
                "xrc_exchange_fetch_total",
                &[("exchange", "Mexc"), ("outcome", "success")],
                10,
                "Per-exchange fetch outcomes.",
            )
            .unwrap();
            e.encode_counter_with_labels(
                "xrc_exchange_fetch_total",
                &[("exchange", "Mexc"), ("outcome", "http_error")],
                3,
                "Per-exchange fetch outcomes.",
            )
            .unwrap();
        });
        let header_lines = out.lines().filter(|l| l.starts_with("# ")).count();
        assert_eq!(header_lines, 2, "expected one HELP + one TYPE line, got:\n{out}");
        assert!(out.contains(r#"xrc_exchange_fetch_total{exchange="Mexc",outcome="success"} 10 0"#));
        assert!(
            out.contains(r#"xrc_exchange_fetch_total{exchange="Mexc",outcome="http_error"} 3 0"#)
        );
    }

    #[test]
    fn labeled_gauge_renders_float_value() {
        let out = encode(0, |e| {
            e.encode_gauge_with_labels(
                "xrc_exchange_last_success_seconds",
                &[("exchange", "Coinbase"), ("kind", "crypto")],
                1_700_000_000.0,
                "Last success timestamp.",
            )
            .unwrap();
        });
        assert!(out.contains(
            r#"xrc_exchange_last_success_seconds{exchange="Coinbase",kind="crypto"} 1700000000 0"#
        ));
    }

    #[test]
    fn label_value_escapes_quotes_backslash_and_newline() {
        let out = encode(0, |e| {
            e.encode_counter_with_labels(
                "metric",
                &[("k", "a\"b\\c\nd")],
                1,
                "h",
            )
            .unwrap();
        });
        assert!(
            out.contains(r#"metric{k="a\"b\\c\nd"} 1 0"#),
            "got: {out}"
        );
    }

    #[test]
    fn empty_label_set_omits_braces() {
        let out = encode(0, |e| {
            e.encode_counter_with_labels("metric", &[], 1, "h").unwrap();
        });
        assert!(out.contains("metric 1 0"), "got: {out}");
        assert!(!out.contains("metric{}"), "got: {out}");
    }

    #[test]
    fn distinct_metric_names_each_get_their_own_header() {
        let out = encode(0, |e| {
            e.encode_counter("a", 1, "ha").unwrap();
            e.encode_counter("b", 2, "hb").unwrap();
        });
        assert!(out.contains("# HELP a ha"));
        assert!(out.contains("# HELP b hb"));
    }

    #[test]
    fn unlabeled_path_does_not_dedupe_headers() {
        // The labeled path shares one header across many series of the same
        // name, but the unlabeled path keeps the historical "always emit"
        // contract so that two unrelated callers can't silently lose a
        // header pair.
        let out = encode(0, |e| {
            e.encode_counter("metric", 1, "help").unwrap();
            e.encode_counter("metric", 2, "help").unwrap();
        });
        let help_lines = out.lines().filter(|l| l.starts_with("# HELP ")).count();
        let type_lines = out.lines().filter(|l| l.starts_with("# TYPE ")).count();
        assert_eq!(help_lines, 2, "got:\n{out}");
        assert_eq!(type_lines, 2, "got:\n{out}");
    }
}
