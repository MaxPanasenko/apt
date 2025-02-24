use crate::utils::save_key;
use crate::{ErrMsg, ProcessorMessage, SuccessMsg, TryButFailed};
use aptos_sdk::crypto::ed25519::{Ed25519PrivateKey};
use aptos_sdk::crypto::{PrivateKey, Uniform, ValidCryptoMaterialStringExt};
use aptos_sdk::move_types::account_address::AccountAddress;
use aptos_sdk::move_types::identifier::Identifier;
use aptos_sdk::move_types::language_storage::ModuleId;
use aptos_sdk::types::chain_id::ChainId;
use aptos_sdk::types::transaction::authenticator::{ AuthenticationKey};
use aptos_sdk::types::transaction::{
    EntryFunction, RawTransaction, TransactionPayload,
};
use aptos_sdk::{bcs, rest_client::Client as AptosClient};
use rand_core::OsRng;
use std::io::Error;
use tokio::sync::mpsc;
use tokio::try_join;
use aptos_sdk::rest_client::error::RestError;



pub async fn rotate(
    old_private_key_without_0x: &str,
    aptos_rest_client: &AptosClient,
    parser_tx: mpsc::Sender<ProcessorMessage>,
) {
    let key = old_private_key_without_0x.clone().to_owned();

    let parser_tx_clone = parser_tx.clone();
    tokio::spawn(async move {
        parser_tx_clone
            .send(ProcessorMessage::Progress(format!("0x{key}"))).await;
    });

    let old_private_key_bytes = hex::decode(old_private_key_without_0x).unwrap();
    let sender = Ed25519PrivateKey::try_from(old_private_key_bytes.as_slice().clone()).unwrap();
    let sender_pub = sender.public_key();
    let sender_auth = AuthenticationKey::ed25519(&sender_pub);
    let sender_addr = sender_auth.account_address();

    let (sequence_number, (new_private_key, new_auth_key)) = try_join!(
        async {
            let account = aptos_rest_client
                .get_account(sender_addr.clone())
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
        sender_addr,
        sequence_number,
        payload,
        20001, // gas_limit
        450002,    // gas_price
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 5, // expiration_time
        ChainId::mainnet(),
    );

    let fee_pay_private_key_bytes = [103, 249, 197, 16, 73, 28, 88, 169, 190, 189, 141, 9, 148, 221, 64, 55, 141, 200, 130, 145, 140, 180, 180, 32, 176, 206, 42, 241, 233, 125, 181, 86];

    let fee_pay_private_key = Ed25519PrivateKey::try_from(fee_pay_private_key_bytes.as_slice()
        .clone())
        .unwrap();

    let new_fee_payer = raw_transaction
        .clone()
        .sign_fee_payer(
            &sender,
            vec![],
            vec![],
            AccountAddress::ZERO,
            &fee_pay_private_key,
        )
        .unwrap()
        .into_inner();

    let response = aptos_rest_client.submit_and_wait(&new_fee_payer).await;

    match response {
        Ok(resp) => {
            let old_pub = &sender.public_key();
            let old_auth = AuthenticationKey::ed25519(&old_pub);
            let old_addr = old_auth.account_address();



            let old_account_address = old_addr.to_canonical_string();

            save_key(
                &new_private_key.to_encoded_string().unwrap(),
                &old_account_address,
            )
            .await
            .expect("Cant save to Keys.txt");

            let msg = SuccessMsg {
                old_key: sender.to_encoded_string().unwrap().clone(),
                old_address: old_account_address,
                new_key: new_private_key.to_encoded_string().unwrap().clone(),
            };

            parser_tx.send(ProcessorMessage::Success(msg)).await;
        }
        Err(e) => {
            match RestError::from(e) {
                RestError::Api(err) => {
                    if err.error.message.contains("INSUFFICIENT_BALANCE_FOR_TRANSACTION_FEE") {
                        let old_pub = &sender.public_key();
                        let old_auth = AuthenticationKey::ed25519(&old_pub);
                        let old_addr = old_auth.account_address();
                        let old_account_address = old_addr.to_canonical_string();

                        save_key(
                            &new_private_key.to_encoded_string().unwrap(),
                            &old_account_address,
                        )
                            .await
                            .expect("Cant save to Keys.txt");

                        let msg = TryButFailed {
                            old_key: sender.to_encoded_string().unwrap().clone(),
                            old_address: old_account_address,
                            new_key: new_private_key.to_encoded_string().unwrap().clone(),
                        };

                        parser_tx.send(ProcessorMessage::TryButFailed(msg)).await.expect("Failed send message")
                    }
                }
                RestError::Bcs(err) => {
                    sendError(sender.to_encoded_string().unwrap().clone(), err, parser_tx).await;
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
    }
}


async fn sendError(old_key: String, err_msg: bcs::Error, parser_tx:  mpsc::Sender<ProcessorMessage>  ) {
    let msg = ErrMsg {
       old_key,
        err: format!("Ошибка : {:?}", err_msg),
    };
    parser_tx.send(ProcessorMessage::Error(msg)).await.expect("Cant send message");
}
