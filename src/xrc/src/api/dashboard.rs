use std::{cell::RefCell, thread::LocalKey};

use ic_xrc_types::{Asset, GetExchangeRateResult};
use serde_bytes::ByteBuf;

use crate::{
    forex::{ForexRatesCollector, FOREX_SOURCES},
    request_log::RequestLog,
    types::HttpResponse,
    DECIMALS, EXCHANGES, FOREX_RATE_COLLECTOR, NONPRIVILEGED_REQUEST_LOG, PRIVILEGED_CANISTER_IDS,
    PRIVILEGED_REQUEST_LOG, RATE_UNIT,
};

const DOCUMENT: &str = r#"
<!DOCTYPE html>
<html lang=\"en\">
    <head>
        <title>Exchange Rate Canister Dashboard</title>
        <style>
            table {
                border: solid;
                text-align: left;
                width: 100%;
                border-width: thin;
            }
            h3 {
                font-variant: small-caps;
                margin-top: 30px;
                margin-bottom: 5px;
            }
            table.forex tr th:first-child {
                width: 25%;
            }
            table td, table th {
                padding: 5px;
            }
            table table { font-size: small; }
            tbody tr:nth-child(odd) { background-color: #eeeeee; }
        </style>
        <script>
            document.addEventListener("DOMContentLoaded", function() {
                var tds = document.querySelectorAll(".ts-class");
                for (var i = 0; i < tds.length; i++) {
                    var td = tds[i];
                    var timestamp = td.textContent * 1000;
                    var date = new Date(timestamp);
                    var options = {
                        year: 'numeric',
                        month: 'short',
                        day: 'numeric',
                        hour: 'numeric',
                        minute: 'numeric',
                        second: 'numeric'
                    };
                    td.title = td.textContent;
                    td.textContent = date.toGMTString(undefined, options);
                }
            });
        </script>
    </head>
    <body>
        <h3>Metadata</h3>
        [METADATA]
        [FOREX_COLLECTOR_STATE]
        <h3>Requests from Privileged Canisters</h3>
        [PRIVILEGED_LOGS]
        <h3>Requests from Other Canisters</h3>
        [NONPRIVILEGED_LOGS]
    </body>
</html>
"#;

const REQUEST_LOG_TABLE: &str = r#"
<table>
    <thead>
        <tr>
            <th>Timestamp</th>
            <th>Canister ID</th>
            <th>Base Asset</th>
            <th>Quote Asset</th>
            <th>Request Timestamp</th>
            <th>Error (if occurred)</th>
            <th>Rate</th>
            <th>Base Asset Received Rates</th>
            <th>Quote Asset Received Rates</th>
            <th>Std dev</th>
            <th>Forex Timestamp</th>
        </tr>
    </thead>
    <tbody>[ROWS]</tbody>
</table>
"#;

const METADATA_TABLE: &str = r#"
<table>
    <tr>
        <th>Decimals</th>
        <td>[DECIMALS]</td>
    </tr>
    <tr>
        <th># of Crypto Exchanges</th>
        <td>[EXCHANGES_NUM]</td>
    </tr>
    <tr>
        <th># of Forex Sources</th>
        <td>[FOREX_SOURCES_NUM]</td>
    </tr>
    <tr>
        <th>Privileged Canister IDs</th>
        <td>
            <table>[PRIVILEGED_CANISTER_IDS]</table>
        </td>
    </tr>
</table>
"#;

pub fn get_dashboard() -> HttpResponse {
    let body = render();
    HttpResponse {
        status_code: 200,
        headers: vec![],
        body: ByteBuf::from(body),
    }
}

fn render() -> Vec<u8> {
    let html = DOCUMENT
        .replace("[METADATA]", &render_metadata())
        .replace("[FOREX_COLLECTOR_STATE]", &render_forex_collectors())
        .replace(
            "[PRIVILEGED_LOGS]",
            &render_request_log_entries(&PRIVILEGED_REQUEST_LOG),
        )
        .replace(
            "[NONPRIVILEGED_LOGS]",
            &render_request_log_entries(&NONPRIVILEGED_REQUEST_LOG),
        );
    html.into_bytes()
}

fn render_metadata() -> String {
    METADATA_TABLE
        .replace("[DECIMALS]", &DECIMALS.to_string())
        .replace(
            "[EXCHANGES_NUM]",
            &EXCHANGES
                .iter()
                .filter(|e| e.is_available())
                .count()
                .to_string(),
        )
        .replace(
            "[FOREX_SOURCES_NUM]",
            &FOREX_SOURCES
                .iter()
                .filter(|s| s.is_available())
                .count()
                .to_string(),
        )
        .replace(
            "[PRIVILEGED_CANISTER_IDS]",
            &PRIVILEGED_CANISTER_IDS
                .iter()
                .map(|id| format!("<tr><td><code>{}</code></td></tr>", id))
                .collect::<Vec<_>>()
                .join(""),
        )
}

fn render_forex_collectors() -> String {
    FOREX_RATE_COLLECTOR.with(|cell| {
        let collector = cell.borrow();
        collector
            .get_timestamps()
            .iter()
            .map(|timestamp| render_forex_collector(&collector, *timestamp))
            .collect::<Vec<_>>()
            .join("")
    })
}

fn render_forex_collector(collector: &ForexRatesCollector, timestamp: u64) -> String {
    let title = format!(
        "<h3>Forex Collection for <span class='ts-class'>{}</span></h3>",
        timestamp
    );
    let table = format!(
        "<table class='forex'><tr><th>Sources</th><td>{}</td></tr></table>",
        collector
            .get_sources(timestamp)
            .unwrap_or_default()
            .join(", ")
    );
    format!("{}{}", title, table)
}

fn render_request_log_entries(log: &'static LocalKey<RefCell<RequestLog>>) -> String {
    let rows = log.with(|cell| {
        cell.borrow()
            .entries()
            .iter()
            .map(|entry| {
                format!(
                    "<tr><td class='ts-class'>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:?}</td>{}</tr>",
                    entry.timestamp,
                    entry.caller,
                    render_asset(&entry.request.base_asset),
                    render_asset(&entry.request.quote_asset),
                    entry.request.timestamp,
                    render_result(&entry.result)
                )
            })
            .collect::<Vec<String>>()
            .join(" ")
    });
    REQUEST_LOG_TABLE.replace("[ROWS]", &rows)
}

fn render_asset(asset: &Asset) -> String {
    let symbol = if asset.symbol.chars().all(char::is_alphanumeric) {
        asset.symbol.clone()
    } else {
        "Invalid Symbol".to_string()
    };

    format!("Symbol: {}<br/>Class: {:?}", symbol, asset.class)
}

fn render_result(result: &GetExchangeRateResult) -> String {
    match result {
        Ok(rate) => format!(
            "<td></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>",
            format_scaled_value(rate.rate),
            rate.metadata.base_asset_num_received_rates,
            rate.metadata.quote_asset_num_received_rates,
            format_scaled_value(rate.metadata.standard_deviation),
            rate.metadata
                .forex_timestamp
                .map(|t| t.to_string())
                .unwrap_or_else(|| "None".to_string())
        ),
        Err(error) => {
            format!(
                "<td>{:?}</td><td></td><td></td><td></td><td></td><td></td>",
                error
            )
        }
    }
}

fn format_scaled_value(value: u64) -> String {
    let fractional = value % RATE_UNIT;
    let whole = value / RATE_UNIT;
    format!(
        "{}.{:0width$}",
        whole,
        fractional,
        width = DECIMALS as usize
    )
}
