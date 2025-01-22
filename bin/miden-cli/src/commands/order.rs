use clap::Parser;
use miden_client::{crypto::FeltRng, notes::{Note, NoteId}, order::OrderInfo, store::AccountRecord, transactions::{TransactionRequest, TransactionRequestBuilder}, Client};

use crate::{create_dynamic_table, utils::{pool::Pool, transaction::execute_transaction}, CLIENT_BINARY_NAME};

#[derive(Default, Debug, Clone, Parser)]
pub struct OrderCmd {
    #[clap(short, long, group = "action", value_name = "ID")]
    show: Option<String>,

    #[clap(short, long, group = "action", value_name = "ID")]
    claim: Option<String>,
}

impl OrderCmd {
    pub async fn execute(&self, mut client: Client<impl FeltRng>) -> Result<(), String> {
        match self {
            OrderCmd { show: Some(id), .. } => show_order(client, id.clone()).await,
            OrderCmd { claim: Some(id), .. } => claim_order(client, id.clone()).await,
            _ => list_orders(client).await,
        }
    }
}

async fn list_orders(_client: Client<impl FeltRng>) -> Result<(), String> {
    Ok(())
}

async fn claim_order(mut client: Client<impl FeltRng>, id: String) -> Result<(), String> {
    let order_note_id = NoteId::try_from_hex(&id).map_err(|err| format!("Invalid note id: {}", err))?;
    let order = client.get_order(order_note_id).await?.ok_or(format!("Order with id {} not found", id))?;

    let order_result = client.find_order_result(order_note_id).await?.ok_or(format!("Order result for order with id {} not found", id))?;

    let transaction_request = TransactionRequestBuilder::new()
        .with_unauthenticated_input_notes(vec![(order_result, None)])
        .build();

    let _ = execute_transaction(
        &mut client,
        order.receiver_account_id().clone(),
        transaction_request,
        false,
        false,
    )
    .await?;

    println!("Order claimed successfully");

    Ok(())
}

async fn show_order(client: Client<impl FeltRng>, id: String) -> Result<(), String> {
    let order_note_id = NoteId::try_from_hex(&id).map_err(|err| format!("Invalid note id: {}", err))?;
    let order = client.get_order(order_note_id).await?;

    if let Some(order) = order {
        let order_result = client.find_order_result(order_note_id).await?;
        let pool_account = client.get_account(order.pool_id().clone()).await?;

        print_order(order.clone(), pool_account)?;

        if let Some(order_result) = order_result {
            print_order_result(order_result.clone())?;

            println!("You can claim the result with command: `{CLIENT_BINARY_NAME} order -c {order_note_id}`");
        }
    } else {
        println!("Order not found");
    }

    Ok(())
}

fn print_order(order: OrderInfo, pool_account: Option<AccountRecord>) -> Result<(), String> {
    println!("Order target pool");
    println!("Order sender: {}", order.receiver_account_id().to_string());

    if let Some(pool_account) = pool_account {

        let pool_account = Pool::from(pool_account.account().clone());
        let mut pool_table = create_dynamic_table(&["Pool", "Reserve In", "Reserve Out", "Nonce"]);
        let pool_vault = pool_account.account().vault();

        let [asset1, asset2] = pool_account.get_pool_supported_assets()?;
        let asset_out_faucet_id = if asset1.eq(order.asset_in_faucet_id()) { asset2 } else { asset1 };

        let reserve_in = pool_vault.get_balance(order.asset_in_faucet_id().clone()).unwrap_or(0);
        let reserve_out = pool_vault.get_balance(asset_out_faucet_id.clone()).unwrap_or(0);

        pool_table.add_row(&[
            pool_account.account().id().to_string(), 
            reserve_in.to_string(), 
            reserve_out.to_string(),
            pool_account.account().nonce().as_int().to_string()
        ]);

        println!("{}", pool_table);
    }

    println!("Order amount");
    
    let mut amount_table = create_dynamic_table(&["Asset", "Amount"]);
    amount_table.add_row(&[
        order.asset_in_faucet_id().to_string(), 
        order.asset_amount().to_string()
    ]);

    println!("{}", amount_table);

    Ok(())
}


fn print_order_result(order_result: Note) -> Result<(), String> {
    println!("Order executed!\n");
    println!("Order result:");
    println!("Result note id: {}", order_result.id().to_string());

    let mut order_result_table = create_dynamic_table(&["Asset", "Amount"]);

    let vault = order_result.assets();
    let asset_out = vault.iter().next().ok_or("Order result has no assets")?.unwrap_fungible();
    order_result_table.add_row(&[
        asset_out.faucet_id().to_string(), 
        asset_out.amount().to_string()
    ]);

    println!("{}", order_result_table);

    Ok(())
}