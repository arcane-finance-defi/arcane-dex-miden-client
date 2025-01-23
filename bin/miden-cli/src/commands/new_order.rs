use clap::Parser;
use miden_client::{
    crypto::FeltRng, order::InsertOrderData, store::AccountFilter, transactions::{CreateOrderTransactionData, TransactionRequestBuilder}, Client
};

use crate::{
    create_dynamic_table, utils::{
        get_input_acc_id_by_prefix_or_default, load_faucet_details_map, parse_account_id, pool::{Pool, pool_account_code_commitment}, transaction::{
            execute_transaction, 
            NoteType
        }, SHARED_TOKEN_DOCUMENTATION
    }
};

#[derive(Debug, Parser, Clone)]
/// Create a new pool account.
pub struct NewOrderCmd {
    #[clap(short = 'i', long = "in", help = SHARED_TOKEN_DOCUMENTATION)]
    /// Defines if the account assets are non-fungible (by default it is fungible).
    asset_in: String,
    #[clap(short = 'o', long = "out", help = "Faucet ID of the output asset")]
    /// Defines if the account assets are non-fungible (by default it is fungible).
    asset_out: String,

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

    #[clap(short = 'p', long = "pool", help = "Pool account ID")]
    pool_account_id: Option<String>,
}

impl NewOrderCmd {
    pub async fn execute(&self, mut client: Client<impl FeltRng>) -> Result<(), String> {
        
        let faucet_details_map = load_faucet_details_map()?;

        let asset_in = faucet_details_map.parse_fungible_asset(&self.asset_in)?;
        
        let asset_out_faucet_id = parse_account_id(&client, self.asset_out.as_str()).await?;

        let asset_out = client.get_account(asset_out_faucet_id).await?;

        let sender_account_id = 
            get_input_acc_id_by_prefix_or_default(&client, self.sender_account_id.clone()).await?;

        if asset_out.is_none() {
            println!("Asset out faucet ID {} not found", asset_out_faucet_id);
            return Ok(());
        }

        if self.pool_account_id.is_none() {
            println!("You have to provide a pool account ID\n Select one from the list");
            let accounts = client.get_account_headers(AccountFilter::CodeCommitment(pool_account_code_commitment())).await?;
            let mut filetered_accounts = vec![];

            for (account_header, _) in accounts {
                let record = client.get_account(account_header.id()).await?.unwrap();
                let pool = Pool::from(record.account().clone());
                if pool.is_asset_supported(asset_in.faucet_id()).unwrap() && pool.is_asset_supported(asset_out_faucet_id).unwrap() {
                    filetered_accounts.push(record.account().clone());
                }
            }


            
            let mut table =
                create_dynamic_table(&["Account ID", "Storage Mode", "Nonce", "Reserves"]);
            for acc in filetered_accounts.iter() {
                let account_vault = acc.vault();

                let reserve_in = account_vault.get_balance(asset_in.faucet_id().clone()).unwrap_or(0);
                let reserve_out = account_vault.get_balance(asset_out_faucet_id.clone()).unwrap_or(0);

                table.add_row(vec![
                    acc.id().to_string(),
                    acc.id().storage_mode().to_string(),
                    acc.nonce().as_int().to_string(),
                    format!("{}/{}", reserve_in, reserve_out)
                ]);
            }

            println!("{table}");
        } else {
            let pool_account_id = parse_account_id(&client, self.pool_account_id.clone().unwrap().as_str()).await?;
            let pool_account = Pool::from(client.get_account(pool_account_id).await?.unwrap().account().clone());

            let asset_out = pool_account.calculate_asset_out(asset_in)?;

            println!("You will receive {} of token {}. (Be aware about the slippage)", asset_out.amount(), asset_out.faucet_id());

            let order_data = CreateOrderTransactionData::new(sender_account_id, asset_in, asset_out.faucet_id());

            let (transaction_request, order_note, tag, recipient) = TransactionRequestBuilder::create_order(
                order_data, 
                (&self.note_type).into(), 
                client.rng()
            )
            .map_err(|err| err.to_string())?;


            let _ = execute_transaction(
                &mut client,
                sender_account_id,
                transaction_request.build(),
                self.force,
                self.delegate_proving,
            )
            .await?;

            let order_data_to_insert = InsertOrderData::new(
                order_note.id(), 
                order_note.nullifier(), 
                pool_account_id, 
                asset_in.faucet_id(), 
                recipient, 
                tag
            );
            client.insert_order(order_data_to_insert).await?;
        }

        Ok(())
    }
}

