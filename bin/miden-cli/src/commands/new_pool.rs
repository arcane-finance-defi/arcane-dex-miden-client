use std::time::Duration;

use clap::{Parser, ValueEnum};
use miden_client::{
    accounts::{
        AccountBuilder, AccountStorageMode, AccountType
    },
    transactions::{FundPoolTransactionData, TransactionRequestBuilder},
    notes::NoteType as MidenNoteType,
    crypto::FeltRng,
    Client,
};
use dex_poc::accounts::pool::PoolAccount;
use tokio::time::sleep;

use crate::{
    commands::new_transactions::execute_transaction, 
    utils::{get_input_acc_id_by_prefix_or_default, load_faucet_details_map, SHARED_TOKEN_DOCUMENTATION}, CLIENT_BINARY_NAME
};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliAccountStorageMode {
    Private,
    Public,
}

impl From<CliAccountStorageMode> for AccountStorageMode {
    fn from(cli_mode: CliAccountStorageMode) -> Self {
        match cli_mode {
            CliAccountStorageMode::Private => AccountStorageMode::Private,
            CliAccountStorageMode::Public => AccountStorageMode::Public,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum NoteType {
    Public,
    Private,
}

impl From<&NoteType> for MidenNoteType {
    fn from(note_type: &NoteType) -> Self {
        match note_type {
            NoteType::Public => MidenNoteType::Public,
            NoteType::Private => MidenNoteType::Private,
        }
    }
}

#[derive(Debug, Parser, Clone)]
/// Create a new pool account.
pub struct NewPoolCmd {
    #[clap(value_enum, short, long, default_value_t = CliAccountStorageMode::Private)]
    /// Storage mode of the account.
    storage_mode: CliAccountStorageMode,
    #[clap(long, help = SHARED_TOKEN_DOCUMENTATION)]
    /// Defines if the account assets are non-fungible (by default it is fungible).
    asset1: String,
    #[clap(long, help = SHARED_TOKEN_DOCUMENTATION)]
    /// Defines if the account assets are non-fungible (by default it is fungible).
    asset2: String,

    #[clap(short, long, value_enum)]
    note_type: NoteType,

    /// Flag to submit the executed transaction without asking for confirmation
    #[clap(long, default_value_t = false)]
    force: bool,

    /// Flag to delegate proving to the remote prover specified in the config file
    #[clap(long, default_value_t = false)]
    delegate_proving: bool,

    #[clap(long = "sender")]
    sender_account_id: Option<String>,
}

impl NewPoolCmd {
    pub async fn execute(&self, mut client: Client<impl FeltRng>) -> Result<(), String> {
        
        let faucet_details_map = load_faucet_details_map()?;

        let asset1 = faucet_details_map.parse_fungible_asset(&self.asset1)?;
        let asset2 = faucet_details_map.parse_fungible_asset(&self.asset2)?;

        let mut init_seed = [0u8; 32];
        client.rng().fill_bytes(&mut init_seed);

        let anchor_block = client.get_latest_epoch_block().await?;

        let (new_account, seed) = AccountBuilder::new()
            .init_seed(init_seed)
            .anchor((&anchor_block).try_into().expect("anchor block should be valid"))
            .account_type(AccountType::RegularAccountImmutableCode)
            .storage_mode(self.storage_mode.into())
            .with_component(
                PoolAccount::new([
                    asset1.faucet_id(), 
                    asset2.faucet_id()
                ])
            )
            .build()
            .map_err(|err| format!("failed to create pool: {}", err))?;

        client
            .add_account(&new_account, Some(seed), None, false)
            .await?;

        println!("Succesfully created new pool.");

        let sender_account_id =
            get_input_acc_id_by_prefix_or_default(&client, self.sender_account_id.clone()).await?;
        let pool_id = new_account.id();

        let fund_data = FundPoolTransactionData::new(pool_id, sender_account_id, [asset1, asset2]);

        let transaction_request = TransactionRequestBuilder::fund_pool(
            fund_data,
            (&self.note_type).into(),
            client.rng(),
        )
        .map_err(|err| err.to_string())?
        .build();

        let output_note = execute_transaction(
            &mut client,
            sender_account_id,
            transaction_request,
            self.force,
            self.delegate_proving,
        )
        .await?;

        println!("Waiting for finalization");
        sleep(Duration::from_secs(10)).await;

        let sync_details = client.sync_state().await?;

        if sync_details.committed_notes.contains(output_note.first().unwrap()) {
            println!("Transaction committed");
        }

        let transaction_request = TransactionRequestBuilder::consume_notes(output_note)
            .without_script()
            .build();

        let _ = execute_transaction(
            &mut client,
            pool_id,
            transaction_request,
            self.force,
            self.delegate_proving,
        )
        .await?;

        println!("Succesfully funded pool.");
        println!(
            "To view account details execute `{CLIENT_BINARY_NAME} account -s {}`",
            pool_id
        );

        Ok(())
    }
}

