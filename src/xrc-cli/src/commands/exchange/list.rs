use serde::Serialize;

#[derive(Serialize)]
struct Exchange {
    name: String,
    host: String,
    path: String,
}

pub fn exec() {
    let exchanges = xrc::EXCHANGES
        .iter()
        .map(|e| {
            let exchange_url = e.get_url("", "", 0);
            let url = url::Url::parse(&exchange_url).expect("Failed to parse url!");
            Exchange {
                name: e.to_string(),
                host: url.host().unwrap().to_string(),
                path: url.path().to_string(),
            }
        })
        .collect::<Vec<_>>();

    println!(
        "{}",
        serde_json::to_string_pretty(&exchanges).expect("Failed to jsonify exchanges")
    )
}
