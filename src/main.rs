mod parser;
mod rotate;
mod tg_bot;
mod utils;

use aptos_sdk::crypto::ed25519::Ed25519PrivateKey;
use crate::utils::get_current_block_height;
use env_logger;
use log::{info};
use tokio::sync::{watch, Mutex};


struct TryButFailed {
    old_key: String,
    old_address: String,
    new_key: String,
}

struct SuccessMsg {
    old_key: String,
    old_address: String,
    new_key: String,
}

struct ErrMsg {
    old_key: String,
    err: String,
}

enum ProcessorMessage {
    Success(SuccessMsg),
    TryButFailed(TryButFailed),
    Progress(String),
    Error(ErrMsg),
}

pub struct AppState {
    pub current_block: u64,
}

impl AppState {
    pub fn new(curr_block: u64) -> Self {
        AppState {
            current_block: curr_block, // Начинаем с блока 0
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Запускаем парсер и бота...");

    let current_block = get_current_block_height().await.unwrap();

    tg_bot::run_bot(current_block).await
}
