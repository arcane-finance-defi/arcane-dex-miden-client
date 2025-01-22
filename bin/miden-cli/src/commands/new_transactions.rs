use clap::Parser;
use miden_client::{
    crypto::FeltRng, notes::{build_swap_tag, get_input_note_with_id_prefix}, transactions::{
        PaymentTransactionData, SwapTransactionData, TransactionRequestBuilder,
    }, Client
};
use tracing::info;

use crate::utils::{
    get_input_acc_id_by_prefix_or_default, load_faucet_details_map, parse_account_id, pool::is_pool_account, transaction::{execute_transaction, NoteType}, SHARED_TOKEN_DOCUMENTATION
};


#[derive(Debug, Parser, Clone)]
/// Mint tokens from a fungible faucet to a wallet.
pub struct MintCmd {
    /// Target account ID or its hex prefix.
    #[clap(short = 't', long = "target")]
    target_account_id: String,

    /// Asset to be minted.
    #[clap(short, long, help=format!("Asset to be minted.\n{SHARED_TOKEN_DOCUMENTATION}"))]
    asset: String,

    #[clap(short, long, value_enum)]
    note_type: NoteType,
    /// Flag to submit the executed transaction without asking for confirmation.
    #[clap(long, default_value_t = false)]
    force: bool,

    /// Flag to delegate proving to the remote prover specified in the config file.
    #[clap(long, default_value_t = false)]
    delegate_proving: bool,
}

impl MintCmd {
    pub async fn execute(&self, mut client: Client<impl FeltRng>) -> Result<(), String> {
        let force = self.force;
        let faucet_details_map = load_faucet_details_map()?;

        let fungible_asset = faucet_details_map.parse_fungible_asset(&self.asset)?;

        let target_account_id = parse_account_id(&client, self.target_account_id.as_str()).await?;

        let transaction_request = TransactionRequestBuilder::mint_fungible_asset(
            fungible_asset,
            target_account_id,
            (&self.note_type).into(),
            client.rng(),
        )
        .map_err(|err| err.to_string())?
        .build();

        let _ = execute_transaction(
            &mut client,
            fungible_asset.faucet_id(),
            transaction_request,
            force,
            self.delegate_proving,
        )
        .await?;

        Ok(())
    }
}

#[derive(Debug, Parser, Clone)]
/// Create a pay-to-id transaction.
pub struct SendCmd {
    /// Sender account ID or its hex prefix. If none is provided, the default account's ID is used
    /// instead.
    #[clap(short = 's', long = "sender")]
    sender_account_id: Option<String>,
    /// Target account ID or its hex prefix.
    #[clap(short = 't', long = "target")]
    target_account_id: String,

    /// Asset to be sent.
    #[clap(short, long, help=format!("Asset to be sent.\n{SHARED_TOKEN_DOCUMENTATION}"))]
    asset: String,

    #[clap(short, long, value_enum)]
    note_type: NoteType,
    /// Flag to submit the executed transaction without asking for confirmation
    #[clap(long, default_value_t = false)]
    force: bool,
    /// Set the recall height for the transaction. If the note wasn't consumed by this height, the
    /// sender may consume it back.
    ///
    /// Setting this flag turns the transaction from a PayToId to a PayToIdWithRecall.
    #[clap(short, long)]
    recall_height: Option<u32>,

    /// Flag to delegate proving to the remote prover specified in the config file
    #[clap(long, default_value_t = false)]
    delegate_proving: bool,
}

impl SendCmd {
    pub async fn execute(&self, mut client: Client<impl FeltRng>) -> Result<(), String> {
        let force = self.force;

        let faucet_details_map = load_faucet_details_map()?;

        let fungible_asset = faucet_details_map.parse_fungible_asset(&self.asset)?;

        // try to use either the provided argument or the default account
        let sender_account_id =
            get_input_acc_id_by_prefix_or_default(&client, self.sender_account_id.clone()).await?;
        let target_account_id = parse_account_id(&client, self.target_account_id.as_str()).await?;

        let payment_transaction = PaymentTransactionData::new(
            vec![fungible_asset.into()],
            sender_account_id,
            target_account_id,
        );

        let transaction_request = TransactionRequestBuilder::pay_to_id(
            payment_transaction,
            self.recall_height,
            (&self.note_type).into(),
            client.rng(),
        )
        .map_err(|err| err.to_string())?
        .build();

        let _ = execute_transaction(
            &mut client,
            sender_account_id,
            transaction_request,
            force,
            self.delegate_proving,
        )
        .await?;

        Ok(())
    }
}

#[derive(Debug, Parser, Clone)]
/// Create a swap transaction.
pub struct SwapCmd {
    /// Sender account ID or its hex prefix. If none is provided, the default account's ID is used
    /// instead.
    #[clap(short = 's', long = "source")]
    sender_account_id: Option<String>,

    /// Asset offered.
    #[clap(long = "offered-asset", help=format!("Asset offered.\n{SHARED_TOKEN_DOCUMENTATION}"))]
    offered_asset: String,

    /// Asset requested.
    #[clap(short, long, help=format!("Asset requested.\n{SHARED_TOKEN_DOCUMENTATION}"))]
    requested_asset: String,

    #[clap(short, long, value_enum)]
    note_type: NoteType,
    /// Flag to submit the executed transaction without asking for confirmation.
    #[clap(long, default_value_t = false)]
    force: bool,

    /// Flag to delegate proving to the remote prover specified in the config file.
    #[clap(long, default_value_t = false)]
    delegate_proving: bool,
}

impl SwapCmd {
    pub async fn execute(&self, mut client: Client<impl FeltRng>) -> Result<(), String> {
        let force = self.force;

        let faucet_details_map = load_faucet_details_map()?;

        let offered_fungible_asset =
            faucet_details_map.parse_fungible_asset(&self.offered_asset)?;
        let requested_fungible_asset =
            faucet_details_map.parse_fungible_asset(&self.requested_asset)?;

        // try to use either the provided argument or the default account
        let sender_account_id =
            get_input_acc_id_by_prefix_or_default(&client, self.sender_account_id.clone()).await?;

        let swap_transaction = SwapTransactionData::new(
            sender_account_id,
            offered_fungible_asset.into(),
            requested_fungible_asset.into(),
        );

        let transaction_request = TransactionRequestBuilder::swap(
            swap_transaction.clone(),
            (&self.note_type).into(),
            client.rng(),
        )
        .map_err(|err| err.to_string())?
        .build();

        execute_transaction(
            &mut client,
            sender_account_id,
            transaction_request,
            force,
            self.delegate_proving,
        )
        .await?;

        let payback_note_tag: u32 = build_swap_tag(
            (&self.note_type).into(),
            &swap_transaction.offered_asset(),
            &swap_transaction.requested_asset(),
        )
        .map_err(|err| err.to_string())?
        .into();
        println!(
            "To receive updates about the payback Swap Note run `miden tags add {}`",
            payback_note_tag
        );

        Ok(())
    }
}

#[derive(Debug, Parser, Clone)]
/// Consume with the account corresponding to `account_id` all of the notes from `list_of_notes`.
/// If no account ID is provided, the default one is used. If no notes are provided, any notes
/// that are identified to be owned by the account ID are consumed.
pub struct ConsumeNotesCmd {
    /// The account ID to be used to consume the note or its hex prefix. If none is provided, the
    /// default account's ID is used instead.
    #[clap(short = 'a', long = "account")]
    account_id: Option<String>,
    /// A list of note IDs or the hex prefixes of their corresponding IDs.
    list_of_notes: Vec<String>,
    /// Flag to submit the executed transaction without asking for confirmation.
    #[clap(short, long, default_value_t = false)]
    force: bool,

    /// Flag to delegate proving to the remote prover specified in the config file.
    #[clap(long, default_value_t = false)]
    delegate_proving: bool,
}

impl ConsumeNotesCmd {
    pub async fn execute(&self, mut client: Client<impl FeltRng>) -> Result<(), String> {
        let force = self.force;

        let mut authenticated_notes = Vec::new();
        let mut unauthenticated_notes = Vec::new();

        for note_id in &self.list_of_notes {
            let note_record = get_input_note_with_id_prefix(&client, note_id)
                .await
                .map_err(|err| err.to_string())?;

            if note_record.is_authenticated() {
                authenticated_notes.push(note_record.id());
            } else {
                unauthenticated_notes.push((note_record.try_into()?, None));
            }
        }

        let account_id =
            get_input_acc_id_by_prefix_or_default(&client, self.account_id.clone()).await?;

        if authenticated_notes.is_empty() {
            info!("No input note IDs provided, getting all notes consumable by {}", account_id);
            let consumable_notes = client.get_consumable_notes(Some(account_id)).await?;

            authenticated_notes.extend(consumable_notes.iter().map(|(note, _)| note.id()));
        }

        let account = client.get_account(account_id).await?;


        if authenticated_notes.is_empty() && unauthenticated_notes.is_empty() {
            return Err(format!("No input notes were provided and the store does not contain any notes consumable by {account_id}"));
        }

        let mut transaction_request_builder = TransactionRequestBuilder::consume_notes(authenticated_notes)
            .with_unauthenticated_input_notes(unauthenticated_notes);


        if let Some(record) = account {
            if is_pool_account(record.account())? {
                transaction_request_builder = transaction_request_builder.without_script();
            }
        }

        let transaction_request = transaction_request_builder.build();

        let _ = execute_transaction(
            &mut client,
            account_id,
            transaction_request,
            force,
            self.delegate_proving,
        )
        .await?;

        Ok(())
    }
}
