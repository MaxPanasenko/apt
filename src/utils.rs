use aptos_sdk::rest_client::Client as AptosClient;
use aptos_sdk::types::account_address::AccountAddress;
use reqwest::Client;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;

pub async fn get_current_block_height() -> Option<u64> {
    let client = Client::builder()
        .pool_idle_timeout(None)
        .pool_max_idle_per_host(100)
        .build()
        .expect("Не удалось создать клиент");

    let url = "https://api.mainnet.aptoslabs.com/v1/".to_string();

    if let Ok(response) = client.get(&url).send().await {
        if let Ok(body) = response.text().await {
            if let Ok(data) = serde_json::from_str::<Value>(&body) {
                return data
                    .get("block_height")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok());
            }
        }
    }
    None
}

pub async fn get_seq_num(rest_client: &AptosClient, address: AccountAddress) -> u64 {
    let account = rest_client.get_account(address).await.unwrap();
    account.inner().sequence_number
}

pub const KEY_FILE: &str = "keys.txt";
pub async fn save_key(key: &str, address: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(KEY_FILE)?;

    file.write_fmt(format_args!(
        "privateKey {}, accountaddress {} \n",
        key, address
    ))?;
    println!("Данные успешно добавлены!");
    Ok(())
}
