use ic_xrc_types::GetExchangeRateResult;
use serde_bytes::ByteBuf;

use crate::{
    forex::FOREX_SOURCES, types::HttpResponse, DECIMALS, EXCHANGES, FOREX_RATE_COLLECTOR,
    PRIVILEGED_CANISTER_IDS, PRIVILEGED_REQUEST_LOG, RATE_UNIT,
};

pub fn get_dashboard() -> HttpResponse {
    let body = render();
    HttpResponse {
        status_code: 200,
        headers: vec![],
        body: ByteBuf::from(body),
    }
}

fn render() -> Vec<u8> {
    let html = format!(
        "
        <!DOCTYPE html>
        <html lang=\"en\">
            <head>
                <title>Exchange Rate Canister Dashboard</title>
                <style>
                    table {{
                        border: solid;
                        text-align: left;
                        width: 100%;
                        border-width: thin;
                    }}
                    h3 {{
                        font-variant: small-caps;
                        margin-top: 30px;
                        margin-bottom: 5px;
                    }}
                    table.forex tr th:first-child {{
                        width: 25%;
                    }}
                    table td, table th {{
                        padding: 5px;
                    }}
                    table table {{ font-size: small; }}
                    tbody tr:nth-child(odd) {{ background-color: #eeeeee; }}
                </style>
                <script>
                    document.addEventListener(\"DOMContentLoaded\", function() {{
                        var tds = document.querySelectorAll(\".ts-class\");
                        for (var i = 0; i < tds.length; i++) {{
                        var td = tds[i];
                        var timestamp = td.textContent * 1000;
                        var date = new Date(timestamp);
                        var options = {{
                            year: 'numeric',
                            month: 'short',
                            day: 'numeric',
                            hour: 'numeric',
                            minute: 'numeric',
                            second: 'numeric'
                        }};
                        td.title = td.textContent;
                        td.textContent = date.toGMTString(undefined, options);
                        }}
                    }});
                </script>
            </head>
            <body>
                <h3>Metadata</h3>
                {}
                {}
                <h3>Requests from Privileged Canisters</h3>
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
                    <tbody>{}</tbody>
                </table>
            </body>
        </html>",
        render_metadata(),
        render_forex_collector(),
        render_request_log_entries()
    );
    html.into_bytes()
}

fn render_metadata() -> String {
    format!(
        "<table>
        <tr>
            <th>Decimals</th>
            <td>{}</td>
        </tr>
        <tr>
            <th># of Crypto Exchanges</th>
            <td>{}</td>
        </tr>
        <tr>
            <th># of Forex Sources</th>
            <td>{}</td>
        </tr>
        <tr>
            <th>Privileged Canister IDs</th>
            <td>
                <table>{}</table>
            </td>
        </tr>
    </table>",
        DECIMALS,
        EXCHANGES.iter().filter(|e| e.is_available()).count(),
        FOREX_SOURCES.iter().filter(|s| s.is_available()).count(),
        PRIVILEGED_CANISTER_IDS
            .iter()
            .map(|id| format!("<tr><td><code>{}</code></td></tr>", id))
            .collect::<Vec<_>>()
            .join("")
    )
}

fn render_forex_collector() -> String {
    FOREX_RATE_COLLECTOR.with(|cell| {
        let collector = cell.borrow();
        collector
            .get_timestamps()
            .iter()
            .map(|timestamp| {
                format!(
                    "<h3>Forex Collection for <span class='ts-class'>{}</span></h3>
                    <table class='forex'>
                        <tr><th>Sources</th><td>{}</td></tr>
                    </table>",
                    timestamp,
                    collector.get_sources(*timestamp).unwrap_or_default().join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join("")
    })
}

fn render_request_log_entries() -> String {
    PRIVILEGED_REQUEST_LOG.with(|cell| {
        cell.borrow()
            .entries()
            .iter()
            .map(|entry| {
                format!(
                    "<tr>
                    <td class='ts-class'>{}</td>
                    <td>{}</td>
                    <td>{:?}</td>
                    <td>{:?}</td>
                    <td>{:?}</td>
                    {}
                </tr>",
                    entry.timestamp,
                    entry.caller,
                    entry.request.base_asset,
                    entry.request.quote_asset,
                    entry.request.timestamp,
                    render_result(&entry.result)
                )
            })
            .collect::<Vec<String>>()
            .join(" ")
    })
}

fn render_result(result: &GetExchangeRateResult) -> String {
    match result {
        Ok(rate) => format!(
            "
            <td></td>
            <td>{}</td>
            <td>{}</td>
            <td>{}</td>
            <td>{}</td>
            <td>{}</td>
        ",
            format_scaled_value(rate.rate),
            rate.metadata.base_asset_num_received_rates,
            rate.metadata.quote_asset_num_received_rates,
            format_scaled_value(rate.metadata.standard_deviation),
            rate.metadata
                .forex_timestamp
                .map(|t| t.to_string())
                .unwrap_or("None".to_string())
        ),
        Err(error) => {
            format!(
                "<td>{:?}</td>
                <td></td>
                <td></td>
                <td></td>
                <td></td>
                <td></td>",
                error
            )
        }
    }
}

fn format_scaled_value(value: u64) -> String {
    let fractional = value % RATE_UNIT;
    let whole = value / RATE_UNIT;
    format!("{}.{:0width$}", whole, fractional, width = DECIMALS as usize)
}