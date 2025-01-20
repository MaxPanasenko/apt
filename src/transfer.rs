use aptos_sdk::coin_client::CoinClient;
use aptos_sdk::crypto::ed25519::Ed25519PrivateKey;
use aptos_sdk::move_types::account_address::AccountAddress;
use aptos_sdk::move_types::identifier::Identifier;
use aptos_sdk::move_types::language_storage::{ModuleId, StructTag, TypeTag};
use aptos_sdk::types::AccountKey;
use aptos_sdk::types::chain_id::ChainId;
use aptos_sdk::types::transaction::{EntryFunction, RawTransaction, SignedTransaction, TransactionPayload};
use aptos_sdk::{bcs, rest_client::{Client as AptosClient, }};
use aptos_sdk::crypto::{PrivateKey, SigningKey};
use crate::utils::get_seq_num;
// transfer_aptos(
//     &aptos_rest_client,
//     old_private_key_str,
//     new_private_key_str
// ).await.expect("Failed");

pub(crate) async fn transfer_aptos(
    client: &AptosClient,
    old_private_key_str: &str,
    new_private_key_str: &str,
) -> Result<(), Error> {
    let main_destination_priv_key = "67f9c510491c58a9bebd8d0994dd40378dc882918cb4b420b0ce2af1e97db556";
    let main_d_bytes = hex::decode(main_destination_priv_key)?;
    let main_d_pk = Ed25519PrivateKey::try_from(main_d_bytes.clone().as_slice())?;
    let main_d_ak = AccountKey::from_private_key(main_d_pk);
    let main_account_address = main_d_ak.authentication_key().account_address();


    let old_private_key_bytes = hex::decode(old_private_key_str)?;
    let old_private_key = Ed25519PrivateKey::try_from(old_private_key_bytes.as_slice().clone())?;
    let old_account_key = AccountKey::from_private_key(old_private_key);
    let old_account_address = old_account_key.authentication_key().account_address().clone();


    let new_private_key_bytes = hex::decode(new_private_key_str)?;
    let new_private_key = Ed25519PrivateKey::try_from(new_private_key_bytes.as_slice().clone())?;

    let coin_client = CoinClient::new(&client);
    let fee = &client.estimate_gas_price().await?.inner().gas_estimate.clone() * 10;
    let sender_balance_apt = coin_client.get_account_balance(&old_account_address).await?;
    let amount = {
        if sender_balance_apt >= fee {
            Ok(sender_balance_apt - fee)
        } else {
            Err("Недостаточно средств для выполнения операции")
        }
    }?;
     if amount {
         let module_address = AccountAddress::from_hex_literal("0x1")?;
         let module_name_transfer = Identifier::new("aptos_account")?;
         let module_name_coin = Identifier::new("aptos_coin")?;
         let name_coin = Identifier::new("AptosCoin")?;
         let fn_name = Identifier::new("transfer_coins")?;
         let module_id = ModuleId::new(module_address.clone(), module_name_transfer);
         let sequence_number = get_seq_num(&client, old_account_address).await;

         let aptos_coin_type = TypeTag::Struct(Box::new(StructTag {
             address: module_address,
             module: module_name_coin,
             name: name_coin,
             type_args: vec![], // Нет вложенных типовых параметров
         }));

         let payload = TransactionPayload::EntryFunction(EntryFunction::new(
             module_id,
             fn_name,
             vec![aptos_coin_type],
             vec![
                 bcs::to_bytes(&main_account_address)?,
                 amount.to_le_bytes().to_vec(),
             ],
         ));

         let raw_transaction = RawTransaction::new(
             old_account_address,
             sequence_number,
             payload,
             10000,
             100,
             std::time::SystemTime::now()
                 .duration_since(std::time::UNIX_EPOCH)?
                 .as_secs()
                 + 30,
             ChainId::mainnet(),
         );

         let sender_public_key = new_private_key.public_key();
         let signature = new_private_key.sign(&raw_transaction)?;
         let signed_transaction = SignedTransaction::new(raw_transaction, sender_public_key, signature);

         let response = client.submit_and_wait(&signed_transaction).await?;
         println!("Транзакция выполнена: {:?}", response);
     }
    Ok(())
}