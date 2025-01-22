use std::{io, sync::Arc};
use clap::ValueEnum;

use miden_client::{
    accounts::AccountId, 
    assets::{FungibleAsset, NonFungibleDeltaAction}, 
    crypto::{Digest, FeltRng}, 
    notes::{NoteId, NoteType as MidenNoteType}, 
    transactions::{TransactionRequest, TransactionResult}, 
    Client
};
use miden_tx_prover::RemoteTransactionProver;

use crate::{create_dynamic_table, utils::{load_config_file, load_faucet_details_map}};

pub async fn execute_transaction(
    client: &mut Client<impl FeltRng>,
    account_id: AccountId,
    transaction_request: TransactionRequest,
    force: bool,
    delegated_proving: bool,
) -> Result<Vec<NoteId>, String> {
    println!("Executing transaction...");
    let transaction_execution_result =
        client.new_transaction(account_id, transaction_request).await?;

    // Show delta and ask for confirmation
    print_transaction_details(&transaction_execution_result)?;
    if !force {
        println!("\nContinue with proving and submission? Changes will be irreversible once the proof is finalized on the rollup (Y/N)");
        let mut proceed_str: String = String::new();
        io::stdin().read_line(&mut proceed_str).expect("Should read line");

        if proceed_str.trim().to_lowercase() != "y" {
            println!("Transaction was cancelled.");
            return Ok(vec![]);
        }
    }

    println!("Proving transaction and then submitting it to node...");

    let transaction_id = transaction_execution_result.executed_transaction().id();
    let output_notes = transaction_execution_result
        .created_notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();

    if delegated_proving {
        let (cli_config, _) = load_config_file()?;
        let remote_prover_endpoint = cli_config
            .remote_prover_endpoint
            .as_ref()
            .ok_or("Remote prover endpoint not found in config file")?;
        let remote_prover =
            Arc::new(RemoteTransactionProver::new(&remote_prover_endpoint.to_string()));
        client
            .submit_transaction_with_prover(transaction_execution_result, remote_prover)
            .await?;
    } else {
        client.submit_transaction(transaction_execution_result).await?;
    }

    println!("Successfully created transaction.");
    println!("Transaction ID: {}", transaction_id);

    if output_notes.is_empty() {
        println!("The transaction did not generate any output notes.");
    } else {
        println!("Output notes:");
        output_notes.iter().for_each(|note_id| println!("\t- {}", note_id));
    }

    Ok(output_notes)
}

fn print_transaction_details(transaction_result: &TransactionResult) -> Result<(), String> {
    println!("The transaction will have the following effects:\n");

    // INPUT NOTES
    let input_note_ids = transaction_result
        .executed_transaction()
        .input_notes()
        .iter()
        .map(|note| note.id())
        .collect::<Vec<_>>();
    if input_note_ids.is_empty() {
        println!("No notes will be consumed.");
    } else {
        println!("The following notes will be consumed:");
        for input_note_id in input_note_ids {
            println!("\t- {}", input_note_id.to_hex());
        }
    }
    println!();

    // OUTPUT NOTES
    let output_note_count = transaction_result.executed_transaction().output_notes().iter().count();
    if output_note_count == 0 {
        println!("No notes will be created as a result of this transaction.");
    } else {
        println!("{output_note_count} notes will be created as a result of this transaction.");
    }
    println!();

    // ACCOUNT CHANGES
    println!(
        "The account with ID {} will be modified as follows:",
        transaction_result.executed_transaction().account_id()
    );

    let account_delta = transaction_result.account_delta();

    let has_storage_changes = !account_delta.storage().is_empty();
    if has_storage_changes {
        let mut table = create_dynamic_table(&["Storage Slot", "Effect"]);

        for (updated_item_slot, new_value) in account_delta.storage().values() {
            let value_digest: Digest = new_value.into();
            table.add_row(vec![
                updated_item_slot.to_string(),
                format!("Updated ({})", value_digest.to_hex()),
            ]);
        }

        println!("Storage changes:");
        println!("{table}");
    } else {
        println!("Account Storage will not be changed.");
    }

    if !account_delta.vault().is_empty() {
        let faucet_details_map = load_faucet_details_map()?;
        let mut table = create_dynamic_table(&["Asset Type", "Faucet ID", "Amount"]);

        for (faucet_id, amount) in account_delta.vault().fungible().iter() {
            let asset = FungibleAsset::new(*faucet_id, amount.unsigned_abs())
                .map_err(|err| err.to_string())?;
            let (faucet_fmt, amount_fmt) = faucet_details_map.format_fungible_asset(&asset)?;

            if amount.is_positive() {
                table.add_row(vec!["Fungible Asset", &faucet_fmt, &format!("+{}", amount_fmt)]);
            } else {
                table.add_row(vec!["Fungible Asset", &faucet_fmt, &format!("-{}", amount_fmt)]);
            }
        }

        for (asset, action) in account_delta.vault().non_fungible().iter() {
            match action {
                NonFungibleDeltaAction::Add => {
                    table.add_row(vec!["Non Fungible Asset", &asset.faucet_id().to_hex(), "1"]);
                },
                NonFungibleDeltaAction::Remove => {
                    table.add_row(vec!["Non Fungible Asset", &asset.faucet_id().to_hex(), "-1"]);
                },
            }
        }

        println!("Vault changes:");
        println!("{table}");
    } else {
        println!("Account Vault will not be changed.");
    }

    if let Some(new_nonce) = account_delta.nonce() {
        println!("New nonce: {new_nonce}.")
    } else {
        println!("No nonce changes.")
    }

    Ok(())
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