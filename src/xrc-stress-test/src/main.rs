use tokio::{process::Command, task};

use candid::{Decode, Encode};
use clap::Parser;

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
    calls_per_round: usize,
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
    println!("calling xrc");
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
    println!("{:#?}", args);
    let wallet_id = get_wallet(&args).await;
    println!("Using wallet ID: {}", wallet_id);

    let mut handles = vec![];
    for _ in 0..args.calls_per_round {
        let cloned_args = args.clone();
        let cloned_wallet_id = wallet_id.clone();
        handles.push(task::spawn(async {
            call_xrc(cloned_args, cloned_wallet_id).await
        }));
    }

    for handle in handles {
        let result = handle.await.expect("future failed");
        println!("{:#?}", result);
    }
}
