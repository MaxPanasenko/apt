use std::any::Any;
use crate::utils::save_key;
use crate::{ErrMsg, ProcessorMessage, SuccessMsg, TryButFailed};
use aptos_sdk::crypto::ed25519::Ed25519PrivateKey;
use aptos_sdk::crypto::{PrivateKey, SigningKey, Uniform, ValidCryptoMaterialStringExt};
use aptos_sdk::move_types::account_address::AccountAddress;
use aptos_sdk::move_types::identifier::Identifier;
use aptos_sdk::move_types::language_storage::ModuleId;
use aptos_sdk::types::chain_id::ChainId;
use aptos_sdk::types::transaction::authenticator::AuthenticationKey;
use aptos_sdk::types::transaction::{
    EntryFunction, RawTransaction, SignedTransaction, TransactionPayload,
};
use aptos_sdk::types::AccountKey;
use aptos_sdk::{bcs, rest_client::Client as AptosClient};
use rand_core::OsRng;
use std::fmt::{format, Debug};
use std::io::Error;
use serde::Deserialize;
use tokio::time::Instant;
use tokio::sync::mpsc;
use tokio::try_join;
use aptos_sdk::rest_client::error::RestError;



pub async fn rotate(
    old_private_key_without_0x: &str,
    aptos_rest_client: &AptosClient,
    parser_tx: mpsc::Sender<ProcessorMessage>,
) {
    let start_time = Instant::now();
    parser_tx
        .send(ProcessorMessage::Progress(format!(
            "`{old_private_key_without_0x:?}`"
        ))).await;
    let old_private_key_bytes = hex::decode(old_private_key_without_0x).unwrap();
    let old_private_key =
        Ed25519PrivateKey::try_from(old_private_key_bytes.as_slice().clone()).unwrap();

    let old_private_key_clone =
        Ed25519PrivateKey::try_from(old_private_key_bytes.as_slice().clone()).unwrap();
    let old_private_key_clone2 =
        Ed25519PrivateKey::try_from(old_private_key_bytes.as_slice().clone()).unwrap();

    let old_account_key_clone = AccountKey::from_private_key(old_private_key);
    let old_account_address = old_account_key_clone.authentication_key().account_address();
    let current_public_key = old_private_key_clone.public_key().clone();

    let (sequence_number, (new_private_key, new_auth_key)) = try_join!(
        async {
            let account = aptos_rest_client
                .get_account(old_account_address)
                .await
                .unwrap();
            Ok::<_, Error>(account.inner().sequence_number)
        },
        async {
            let new_private_key = Ed25519PrivateKey::generate(&mut OsRng);
            let new_public_key = new_private_key.public_key();
            let new_auth_key = AuthenticationKey::ed25519(&new_public_key);

            Ok::<_, Error>((new_private_key, new_auth_key))
        }
    )
    .expect("Cant generate new wallet");

    println!(
        "new_private_key - {:?}",
        &new_private_key.to_encoded_string().unwrap()
    );
    let module_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let module_name = Identifier::new("account").unwrap();
    let module_id = ModuleId::new(module_address, module_name);
    let fn_name = Identifier::new("rotate_authentication_key_call").unwrap();

    let payload = TransactionPayload::EntryFunction(EntryFunction::new(
        module_id,
        fn_name,
        vec![],
        vec![bcs::to_bytes(&new_auth_key).unwrap()],
    ));

    let raw_transaction = RawTransaction::new(
        old_account_address,
        sequence_number,
        payload,
        100000, // gas_limit
        200,    // gas_price
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 20, // expiration_time
        ChainId::mainnet(),
    );

    let signature = old_private_key_clone2.sign(&raw_transaction).unwrap();
    let signed_transaction = SignedTransaction::new(raw_transaction, current_public_key, signature);

    let response = aptos_rest_client.submit_and_wait(&signed_transaction).await;

    match response {
        Ok(resp) => {
            let old_private_key_bytes = hex::decode(old_private_key_without_0x).unwrap();
            let old_private_key =
                Ed25519PrivateKey::try_from(old_private_key_bytes.as_slice().clone()).unwrap();
            let old_account_key_clone = AccountKey::from_private_key(old_private_key);
            let old_account_address = old_account_key_clone
                .authentication_key()
                .account_address()
                .to_canonical_string();
            let elapsed_time = start_time.elapsed();
            println!("Время выполнения транзакции: {:.2?}", elapsed_time);
            save_key(
                &new_private_key.to_encoded_string().unwrap(),
                &old_account_address,
            )
            .await
            .expect("Cant save to Keys.txt");

            let msg = SuccessMsg {
                old_key: old_private_key_clone2.to_encoded_string().unwrap().clone(),
                old_address: old_account_address.clone(),
                new_key: new_private_key.to_encoded_string().unwrap().clone(),
            };

            parser_tx.send(ProcessorMessage::Success(msg)).await;
        }
        Err(e) => {
            match RestError::from(e) {
                RestError::Api(err) => {
                    if err.error.message.contains("INSUFFICIENT_BALANCE_FOR_TRANSACTION_FEE") {
                        let old_private_key_bytes = hex::decode(old_private_key_without_0x).unwrap();
                        let old_private_key =
                            Ed25519PrivateKey::try_from(old_private_key_bytes.as_slice().clone()).unwrap();
                        let old_account_key_clone = AccountKey::from_private_key(old_private_key);
                        let old_account_address = old_account_key_clone
                            .authentication_key()
                            .account_address()
                            .to_canonical_string();
                        let elapsed_time = start_time.elapsed();
                        println!("Время выполнения транзакции: {:.2?}", elapsed_time);
                        save_key(
                            &new_private_key.to_encoded_string().unwrap(),
                            &old_account_address,
                        )
                            .await
                            .expect("Cant save to Keys.txt");

                        let msg = TryButFailed {
                            old_key: old_private_key_clone2.to_encoded_string().unwrap().clone(),
                            old_address: old_account_address.clone(),
                            new_key: new_private_key.to_encoded_string().unwrap().clone(),
                        };

                        parser_tx.send(ProcessorMessage::TryButFailed(msg)).await.expect("Failed send message")
                    }
                }
                RestError::Bcs(err) => {
                    sendError(old_private_key_clone2.to_encoded_string().unwrap().clone(), err, parser_tx).await;
                }
                RestError::Json(_) => {
                }
                RestError::UrlParse(_) => {
                }
                RestError::Timeout(_) => {}
                RestError::Unknown(_) => {}
                RestError::Http(_, _) => {}
            }


        }
        _ => {}
    }
}


async fn sendError(old_key: String, err_msg: bcs::Error, parser_tx:  mpsc::Sender<ProcessorMessage>  ) {
    let msg = ErrMsg {
        old_key,
        err: format!("Ошибка : {:?}", err_msg),
    };
    parser_tx.send(ProcessorMessage::Error(msg)).await.expect("Cant send message");
}
