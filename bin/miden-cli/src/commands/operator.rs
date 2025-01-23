use clap::Parser;
use miden_client::{accounts::AccountId, crypto::FeltRng, order::OrderInfo, store::{AccountFilter, OrderFilter}, transactions::TransactionRequestBuilder, Client};

use crate::{create_dynamic_table, utils::{pool::{calculate_amount_out, pool_account_code_commitment, Pool}, transaction::execute_transaction}};

#[derive(Default, Debug, Clone, Parser)]
pub struct OperatorCmd {
    #[clap(short, long, group = "action", value_name = "ID", help = "Show pool pending orders")]
    show: Option<String>,

    #[clap(short, long, group = "action", value_name = "ID", help = "Process pool pending orders")]
    process: Option<String>,

    #[clap(short, long, group = "action")]
    list: bool,

    /// Flag to submit the executed transaction without asking for confirmation.
    #[clap(long, default_value_t = false)]
    force: bool,

    /// Flag to delegate proving to the remote prover specified in the config file.
    #[clap(long, default_value_t = false)]
    delegate_proving: bool,
}

impl OperatorCmd {
    pub async fn execute(&self, client: Client<impl FeltRng>) -> Result<(), String> {
        match self {
            OperatorCmd { show: Some(pool_id), .. } => show_pool(client, pool_id.clone()).await,
            OperatorCmd { process: Some(pool_id), .. } => process_pool(client, pool_id.clone(), self.force, self.delegate_proving).await,
            OperatorCmd { list: true, .. } => list_pools(client).await,
            _ => list_pools(client).await,
        }
    }
}

async fn process_pool(mut client: Client<impl FeltRng>, pool_id: String, force: bool, delegate_proving: bool) -> Result<(), String> {
    let pool_id = AccountId::from_hex(&pool_id).map_err(|err| format!("Invalid pool id: {}", err)).expect("Invalid pool id");
    let pool = Pool::from(
        client.get_account(pool_id).await
            .expect(format!("Pool with id {} not found", pool_id).as_str())
            .expect("Failed to get pool account")
            .account().clone()
    );

    let [asset1, asset2] = pool.get_pool_supported_assets()?;

    println!("Syncing state...");
    client.sync_state().await.map_err(|err| format!("Failed to sync state: {}", err))?;
    println!("State synced");

    let orders = client.get_orders(pool_id, OrderFilter::Pending).await?;

    let mut execution_table = create_dynamic_table(&["Order ID", "Asset In Faucet ID", "Amount In", "Asset Out Faucet ID", "Amount Out"]);

    let mut input_notes = vec![];

    let mut reserve1 = pool.account().vault().get_balance(asset1).map_err(|err| format!("Failed to get reserve 1 balance: {}", err))?;
    let mut reserve2 = pool.account().vault().get_balance(asset2).map_err(|err| format!("Failed to get reserve 2 balance: {}", err))?;

    for order in orders {
        let input_note = order.note_id().clone();

        let amount_in = order.asset_amount();
        let reserve_in = if asset1.eq(&order.asset_in_faucet_id()) { reserve1 } else { reserve2 };
        let reserve_out = if asset1.eq(&order.asset_in_faucet_id()) { reserve2 } else { reserve1 };

        let amount_out = calculate_amount_out(reserve_in, reserve_out, amount_in.clone());
        let asset_out = if asset1.eq(&order.asset_in_faucet_id()) { asset2 } else { asset1 };

        let reserve_in = reserve_in + amount_in;
        let reserve_out = reserve_out - amount_out;

        reserve1 = if asset1.eq(&order.asset_in_faucet_id()) { reserve_in } else { reserve_out };
        reserve2 = if asset1.eq(&order.asset_in_faucet_id()) { reserve_out } else { reserve_in };
        
        input_notes.push(input_note);

        execution_table.add_row([
            order.note_id().clone().to_hex(), 
            order.asset_in_faucet_id().clone().to_hex(), 
            order.asset_amount().clone().to_string(), 
            asset_out.clone().to_hex(), 
            amount_out.clone().to_string()
        ]);
    }

    println!("{}", execution_table);

    let transaction_request = TransactionRequestBuilder::new()
        .with_authenticated_input_notes(input_notes.into_iter().map(|note| (note.clone(), None)))
        .without_script()
        .build();

    let _ = execute_transaction(
        &mut client,
        pool_id,
        transaction_request,
        force,
        delegate_proving,
    )
    .await?;

    Ok(())
}

async fn list_pools(client: Client<impl FeltRng>) -> Result<(), String> {
    let pool_headers = client.get_account_headers(AccountFilter::CodeCommitment(pool_account_code_commitment())).await?;
    let mut pools = vec![];

    for (pool_header, _) in pool_headers {
        let pool = Pool::from(
            client.get_account(pool_header.id()).await?.ok_or(format!("Pool with id {} not found", pool_header.id()))?.account().clone()
        );
        pools.push(pool);
    }

    describe_pools(pools)?;

    Ok(())
}

async fn show_pool(client: Client<impl FeltRng>, pool_id: String) -> Result<(), String> {
    let pool_id = AccountId::from_hex(&pool_id).map_err(|err| format!("Invalid pool id: {}", err))?;
    let pool = Pool::from(client.get_account(pool_id).await?.ok_or(format!("Pool with id {} not found", pool_id))?.account().clone());

    println!("Pool");
    describe_pools(vec![pool])?;

    let orders = client.get_orders(pool_id, OrderFilter::Pending).await?;

    println!("Found {} pending orders", orders.len());

    if orders.is_empty() {
        return Ok(());
    }

    describe_orders(orders)?;

    Ok(())
}

fn describe_pools(pools: Vec<Pool>) -> Result<(), String> {
    let mut pool_table = create_dynamic_table(&["Pool ID", "Asset 1", "Asset 2", "Reserve 1", "Reserve 2"]);

    for pool in pools {
        let [asset1, asset2] = pool.get_pool_supported_assets().map_err(|err| format!("Failed to get pool supported assets: {}", err))?;

        let reserve1 = pool.account().vault().get_balance(asset1).map_err(|err| format!("Failed to get reserve 1 balance: {}", err))?;
        let reserve2 = pool.account().vault().get_balance(asset2).map_err(|err| format!("Failed to get reserve 2 balance: {}", err))?;

        pool_table.add_row([pool.account().id().to_hex(), asset1.to_hex(), asset2.to_hex(), reserve1.to_string(), reserve2.to_string()]);
    }

    println!("{}", pool_table);

    Ok(())
}

fn describe_orders(orders: Vec<OrderInfo>) -> Result<(), String> {
    let mut order_table = create_dynamic_table(&["Order ID", "Asset In Faucet ID", "Amount"]);

    for order in orders {
        order_table.add_row([
            order.note_id().to_hex(), 
            order.asset_in_faucet_id().to_hex(),
            order.asset_amount().to_string()
        ]);
    }

    println!("{}", order_table);

    Ok(())
}