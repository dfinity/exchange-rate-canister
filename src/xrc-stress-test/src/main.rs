use candid::{Decode, Encode};
use clap::Parser;
use std::cell::{Cell, RefCell};
use tokio::{process::Command, task, time::Instant};

thread_local! {
    static RATE_RESULTS: RefCell<Vec<xrc::candid::GetExchangeRateResult>> = RefCell::new(Vec::new());
    static RESPONSE_TIMES: RefCell<Vec<u128>> = RefCell::new(Vec::new());
    static REPLICA_ERRORS_COUNTER: Cell<usize> = Cell::new(0);
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum AssetClass {
    Crypto,
    Fiat,
}

impl Default for AssetClass {
    fn default() -> Self {
        Self::Crypto
    }
}

impl From<AssetClass> for xrc::candid::AssetClass {
    fn from(arg_class: AssetClass) -> Self {
        match arg_class {
            AssetClass::Crypto => xrc::candid::AssetClass::Cryptocurrency,
            AssetClass::Fiat => xrc::candid::AssetClass::FiatCurrency,
        }
    }
}

#[derive(Parser, Clone, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = String::from("ICP"))]
    base_asset_symbol: String,
    #[arg(short, long, default_value_t = String::from("USDT"))]
    quote_asset_symbol: String,
    #[arg(long, value_enum, default_value_t)]
    base_asset_class: AssetClass,
    #[arg(long, value_enum, default_value_t)]
    quote_asset_class: AssetClass,
    #[arg(short, long)]
    timestamp_secs: Option<u64>,
    #[arg(short, long, default_value_t = 1)]
    calls: usize,
    #[arg(short, long, default_value_t = String::from("local"))]
    network: String,
}

async fn get_wallet(args: &Args) -> String {
    let output = Command::new("dfx")
        .args(["identity", "--network", &args.network, "get-wallet"])
        .output()
        .await
        .expect("failed to get wallet id");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if stdout.is_empty() {
        println!("{}", stderr);
    }
    stdout.trim().to_string()
}

async fn call_xrc(args: Args, wallet_id: String) -> xrc::candid::GetExchangeRateResult {
    let request = xrc::candid::GetExchangeRateRequest {
        base_asset: xrc::candid::Asset {
            symbol: args.base_asset_symbol.clone(),
            class: args.base_asset_class.into(),
        },
        quote_asset: xrc::candid::Asset {
            symbol: args.quote_asset_symbol.clone(),
            class: args.quote_asset_class.into(),
        },
        timestamp: args.timestamp_secs,
    };

    let bytes = Encode!(&request).expect("Failed to encode the request.");
    let raw = hex::encode(bytes);

    let output = Command::new("dfx")
        .args([
            "canister",
            "--network",
            &args.network,
            "call",
            "--with-cycles",
            "10000000000",
            "--wallet",
            &wallet_id,
            "--type",
            "raw",
            "--output",
            "raw",
            "xrc",
            "get_exchange_rate",
            &raw,
        ])
        .output()
        .await
        .expect("Failed to call xrc");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stdout.is_empty() {
        println!("{}", stderr);
    }

    let result_bytes = hex::decode(stdout).expect("failed to decode hex blob");
    let result = Decode!(&result_bytes, xrc::candid::GetExchangeRateResult)
        .expect("failed to decode result from bytes");

    result
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    println!("Args: {:#?}", args);
    let wallet_id = get_wallet(&args).await;
    println!("Using wallet ID: {}", wallet_id);

    let mut handles = vec![];
    for _ in 0..args.calls {
        let cloned_args = args.clone();
        let cloned_wallet_id = wallet_id.clone();
        handles.push(task::spawn(async {
            let now = Instant::now();
            let result = call_xrc(cloned_args, cloned_wallet_id).await;
            (now.elapsed().as_millis(), result)
        }));
    }

    for handle in handles {
        let result = handle.await;
        match result {
            Ok((response_time_ms, exchange_rate_result)) => {
                RESPONSE_TIMES.with(|c| c.borrow_mut().push(response_time_ms));
                RATE_RESULTS.with(|c| c.borrow_mut().push(exchange_rate_result));
            }
            Err(error) => {
                REPLICA_ERRORS_COUNTER.with(|c| c.set(c.get() + 1));
                println!("{}", error);
            }
        };
    }

    RESPONSE_TIMES.with(|c| {
        let mut response_times = c.borrow_mut();
        response_times.sort_unstable();
        println!("{:#?}", response_times);

        let p50 = calculate_p50(&response_times);
        let p90 = calculate_p90(&response_times);
        let p95 = calculate_p95(&response_times);
        println!("p50: {}ms", p50);
        println!("p90: {}ms", p90);
        println!("p95: {}ms", p95);
    });
    REPLICA_ERRORS_COUNTER.with(|c| println!("{:#?}", c.get()));
}

fn calculate_p50(values: &[u128]) -> u128 {
    let index = (values.len() as f64 * 0.5) as usize;
    values[index - 1]
}

fn calculate_p90(values: &[u128]) -> u128 {
    let index = (values.len() as f64 * 0.9) as usize;
    values[index - 1]
}

fn calculate_p95(values: &[u128]) -> u128 {
    let index = (values.len() as f64 * 0.95) as usize;
    values[index - 1]
}
