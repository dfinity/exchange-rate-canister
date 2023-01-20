mod types;

use candid::{Decode, Encode, Nat, Principal};
use clap::{arg, Parser};
use ic_agent::{agent::http_transport::ReqwestHttpReplicaV2Transport, Agent};

#[derive(Parser, Debug)]
struct Arguments {
    #[arg(short, long)]
    url: Option<String>,
    #[arg(short, long)]
    canister_id: String,
}

const IC_URL: &str = "https://ic0.app";
const ENTRIES_LIMIT: usize = 1000;

struct Canister {
    id: Principal,
    agent: Agent,
}

impl Canister {
    async fn new(url: String, id: Principal) -> Self {
        let transport =
            ReqwestHttpReplicaV2Transport::create(&url).expect("Failed to create transport");
        let agent = Agent::builder()
            .with_transport(transport)
            .build()
            .expect("Failed to create agent.");

        if url != IC_URL {
            agent
                .fetch_root_key()
                .await
                .expect("Failed to fetch root key");
        }

        Self { id, agent }
    }

    async fn get_entries(&self, offset: usize) -> monitor_canister::types::GetEntriesResponse {
        let request = monitor_canister::types::GetEntriesRequest {
            offset: Nat::from(offset),
            limit: Some(Nat::from(ENTRIES_LIMIT)),
        };
        let arg = Encode!(&request).expect("Failed to encode `get_entries` request");
        let bytes = self
            .agent
            .update(&self.id, "get_entries")
            .with_arg(&arg)
            .call_and_wait()
            .await
            .expect("Failed to call canister");
        let response = Decode!(&bytes, monitor_canister::types::GetEntriesResponse)
            .expect("Failed to decode `get_entries` response");
        response
    }
}

#[tokio::main]
async fn main() {
    let args = Arguments::parse();
    let url = args.url.unwrap_or_else(|| IC_URL.to_string());
    let canister_id = Principal::from_text(&args.canister_id).expect("Invalid canister ID");
    let canister = Canister::new(url, canister_id).await;
    let mut total_entries_len: usize = 0;
    loop {
        let response = canister.get_entries(total_entries_len).await;
        let entries_len = response.entries.len();

        let entries = response
            .entries
            .into_iter()
            .map(|e| types::Entry::from(e))
            .collect::<Vec<types::Entry>>();

        let bytes = serde_json::to_string_pretty(&entries).unwrap();
        std::fs::write(&format!("./{}.json", total_entries_len), bytes).unwrap();

        total_entries_len += entries_len;
        if entries_len < ENTRIES_LIMIT {
            break;
        }
    }
}
