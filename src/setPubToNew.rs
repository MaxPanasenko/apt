// let rotate_2 =  "fe75afb686677bf21ea737be29737b7e98a7354b3708cbe26321c9a5b83e7586";
// let current_key_rotate_2 = hex::decode(rotate_2)?;
// // let current_key_rotate_2_copy = hex::decode(rotate_2)?;
// let current_key_rotate_2_pk = Ed25519PrivateKey::try_from(current_key_rotate_2.as_slice())?;
// let current_key_rotate_2_pk_copy = Ed25519PrivateKey::try_from(current_key_rotate_2.as_slice())?;
// let current_key_rotate_2_pk_copy_2 = Ed25519PrivateKey::try_from(current_key_rotate_2.as_slice())?;
//
// let current_account_rotate_2_format = AccountKey::from_private_key(current_key_rotate_2_pk);
// let current_account_rotate_2_format_2 = AccountKey::from_private_key(current_key_rotate_2_pk_copy_2);
// let account_address_rotate_2 = current_account_rotate_2_format.authentication_key().account_address();
// println!("addr: {:?}", &account_address_rotate_2.to_canonical_string());
//
//
//
// let fn_name = Identifier::new("set_originating_address")?;
//
// let payload = TransactionPayload::EntryFunction(EntryFunction::new(
// module_id, // Адрес модуля
// fn_name,                   // Имя модуля
// vec![],
// vec![], // Новый authentication_key
// ));
//
// let raw_transaction = RawTransaction::new(
// account_address_rotate_2,
// 0,
// payload,
// 10000, // gas_limit
// 100,     // gas_price
// std::time::SystemTime::now()
// .duration_since(std::time::UNIX_EPOCH)?
// .as_secs()
// + 60, // expiration_time
// ChainId::mainnet(),
// );
//
// let signature = current_key_rotate_2_pk_copy.sign(&raw_transaction)?;
// let current_account_pub_key = current_account_rotate_2_format_2.public_key().clone();
// // faucet_client.fund(account_address_rotate_2, 100000000).await?;
// // println!("Аккаунт пополнен.");
// let signed_transaction = SignedTransaction::new(
// raw_transaction,
// current_account_pub_key,
// signature,
// );
//
// let response = aptos_rest_client.submit_and_wait(&signed_transaction).await?;
// println!("Транзакция выполнена: {:?}", response);
// let elapsed_time = start_time.elapsed();
// println!("Время выполнения транзакции: {:.2?}", elapsed_time);
// Ok(())