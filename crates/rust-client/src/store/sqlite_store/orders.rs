
use alloc::{
    rc::Rc,
    string::ToString,
    vec::Vec,
    string::String,
};

use dex_poc::notes::build_swap_result_from_parts;
use miden_objects::{
    accounts::AccountId, crypto::utils::{Deserializable, Serializable}, notes::{Note, NoteAssets, NoteId, NoteTag}, Digest, Word
};

use rusqlite::{params, types::Value, Connection};

use crate::{order::{InsertOrderData, OrderInfo}, store::{NoteRecordError, OrderFilter, StoreError}};

use super::SqliteStore;

impl SqliteStore {

    pub(crate) fn insert_order(conn: &mut Connection, data: InsertOrderData) -> Result<(), StoreError> {
        const QUERY: &str = "INSERT INTO orders (note_id, note_nullifier, pool_id, asset_in_faucet_id, recipient_serial_number, recipient_receiver_account_id, response_note_tag) VALUES (?, ?, ?, ?, ?, ?, ?)";

        let receiver_account_id_felts = data.response_recipient().inputs().values();
        conn.execute(QUERY, params![
            Rc::new(Value::Text(data.note_id().to_string())), 
            Rc::new(Value::Text(data.note_nullifier().inner().to_string())), 
            Rc::new(Value::Text(data.pool_id().to_string())), 
            Rc::new(Value::Text(data.asset_in_faucet_id().to_string())),
            Rc::new(Value::Text(Digest::from(data.response_recipient().serial_num()).to_string())),
            Rc::new(Value::Text(AccountId::try_from([receiver_account_id_felts[1], receiver_account_id_felts[0]])?.to_string())),
            Rc::new(Value::Blob(data.response_note_tag().to_bytes())),
        ]).map_err(|err| StoreError::DatabaseError(err.to_string())).map(|_| ())
    }

    pub(crate) fn find_order_result(conn: &mut Connection, order_note_id: NoteId) -> Result<Option<Note>, StoreError> {
        let tx = conn.transaction()?;

        const FIND_ORDER_QUERY: &str = "SELECT recipient_serial_number, recipient_receiver_account_id, response_note_tag, pool_id FROM orders WHERE note_id = ?";

        let order_result = parse_order_expected_response_result(tx.prepare(FIND_ORDER_QUERY)?
            .query_row(params![order_note_id.to_string()], parse_order_expected_response_columns)?
        )?;

        const FIND_OUTPUT_NOTE_ASSETS_QUERY: &str = "SELECT assets FROM output_notes WHERE tag = ?";

        let assets = parse_assets(
            tx.query_row(FIND_OUTPUT_NOTE_ASSETS_QUERY, params![order_result.response_note_tag.to_bytes()], parse_assets_column)
        )?;

        tx.commit()?;

        if let Some(assets) = assets {
            return build_swap_result_from_parts(
                order_result.receiver_account_id, 
                order_result.serial_number, 
                assets, 
                order_result.pool_id, 
                order_result.response_note_tag
            )
            .map(|note| Some(note))
            .map_err(|err| NoteRecordError::NoteError(err).into());
        } else {
            Ok(None)
        }

    }

    pub(crate) fn get_order(conn: &mut Connection, order_note_id: NoteId) -> Result<Option<OrderInfo>, StoreError> {
        get_order(conn, order_note_id)
    }

    pub(crate) fn get_orders(conn: &mut Connection, pool_id: AccountId, filter: OrderFilter) -> Result<Vec<OrderInfo>, StoreError> {
        let ids = get_order_ids(conn, pool_id, filter)?;
        let mut orders = vec![];
        for id in ids {
            let order = get_order(conn, id)?.ok_or(StoreError::DatabaseError(String::from("Order not found")))?;
            orders.push(order);
        }
        Ok(orders)
    }

    
    
}

fn get_order_ids(conn: &mut Connection, pool_id: AccountId, filter: OrderFilter) -> Result<Vec<NoteId>, StoreError> {
    match filter {
        OrderFilter::Pending => {
            const FIND_PENDING_ORDERS_QUERY: &str = "SELECT o.note_id FROM orders o LEFT JOIN output_notes n ON n.tag = o.response_note_tag WHERE o.pool_id = ? AND n.tag IS NULL";
            let ids = conn
                .prepare(FIND_PENDING_ORDERS_QUERY)?
                .query_map(params![pool_id.to_string()], |row| row.get::<usize, String>(0))?
                .map(|id| if id.is_ok() { 
                    NoteId::try_from_hex(id.unwrap().as_str()).map_err(|err| StoreError::DatabaseError(err.to_string())) 
                } else { 
                    Err(StoreError::DatabaseError(id.unwrap_err().to_string())) 
                })
                .collect::<Result<Vec<NoteId>, StoreError>>()?;

            Ok(ids)
        },
        OrderFilter::All => {
            const FIND_ALL_ORDERS_QUERY: &str = "SELECT note_id FROM orders WHERE pool_id = ?";
            let ids = conn
                .prepare(FIND_ALL_ORDERS_QUERY)?
                .query_map(params![pool_id.to_string()], |row| row.get::<usize, String>(0))?
                .map(|id| if id.is_ok() { 
                    NoteId::try_from_hex(id.unwrap().as_str()).map_err(|err| StoreError::DatabaseError(err.to_string())) 
                } else { 
                    Err(StoreError::DatabaseError(id.unwrap_err().to_string())) 
                })
                .collect::<Result<Vec<NoteId>, StoreError>>()?;
            
            Ok(ids)
        }
    }
}

fn get_order(conn: &mut Connection, order_note_id: NoteId) -> Result<Option<OrderInfo>, StoreError> {
    let tx = conn.transaction()?;
    const FIND_ORDER_QUERY: &str = "SELECT pool_id, asset_in_faucet_id, recipient_receiver_account_id, recipient_serial_number, response_note_tag FROM orders WHERE note_id = ?";

    let order_ids = parse_order_ids(tx.prepare(FIND_ORDER_QUERY)?
        .query_row(params![order_note_id.to_string()], parse_order_ids_columns)?
    ).map_or(None, |order_ids| Some(order_ids));

    if let Some(order_ids) = order_ids {

        const FIND_OUTPUT_NOTE_ASSETS_QUERY: &str = "SELECT assets FROM output_notes WHERE note_id = ?";

        let assets = parse_assets(
            tx.query_row(FIND_OUTPUT_NOTE_ASSETS_QUERY, params![order_note_id.to_string()], parse_assets_column)
        )?;

        tx.commit()?;

        if let Some(assets) = assets {
            Ok(Some(OrderInfo::new(
                order_note_id,
                order_ids.pool_id, 
                order_ids.recipient_receiver_account_id,
                order_ids.recipient_serial_number,
                order_ids.response_note_tag,
                order_ids.asset_in_faucet_id, 
                assets.iter().next().unwrap().unwrap_fungible().amount()
            )))
        } else {
            Ok(None)
        }

    } else {
        tx.commit()?;

        Ok(None)
    }

}

struct OrderIdsParsed {
    pool_id: String,
    asset_in_faucet_id: String,
    recipient_receiver_account_id: String,
    recipient_serial_number: String,
    response_note_tag: Vec<u8>,
}

struct OrderIds {
    pool_id: AccountId,
    asset_in_faucet_id: AccountId,
    recipient_receiver_account_id: AccountId,
    recipient_serial_number: Word,
    response_note_tag: NoteTag,
}

fn parse_order_ids_columns(row: &rusqlite::Row<'_>) -> Result<OrderIdsParsed, rusqlite::Error> {
    let pool_id: String = row.get(0)?;
    let asset_in_faucet_id: String = row.get(1)?;
    let recipient_receiver_account_id: String = row.get(2)?;
    let recipient_serial_number: String = row.get(3)?;
    let response_note_tag: Vec<u8> = row.get(4)?;
    Ok(OrderIdsParsed { pool_id, asset_in_faucet_id, recipient_receiver_account_id, recipient_serial_number, response_note_tag })
}

fn parse_order_ids(parsed: OrderIdsParsed) -> Result<OrderIds, StoreError> {
    Ok(OrderIds { 
        pool_id: AccountId::from_hex(&parsed.pool_id)?, 
        asset_in_faucet_id: AccountId::from_hex(&parsed.asset_in_faucet_id)?,
        recipient_receiver_account_id: AccountId::from_hex(&parsed.recipient_receiver_account_id)?,
        recipient_serial_number: Digest::try_from(&parsed.recipient_serial_number)?.into(),
        response_note_tag: NoteTag::read_from_bytes(&parsed.response_note_tag)?,
    })
}

struct OrderNoteExpectedResponseInfoParsed {
    serial_number: String,
    receiver_account_id: String,
    response_note_tag: Vec<u8>,
    pool_id: String,
}

struct OrderNoteExpectedResponseInfo {
    serial_number: Word,
    receiver_account_id: AccountId,
    response_note_tag: NoteTag, 
    pool_id: AccountId,
}

fn parse_order_expected_response_columns(row: &rusqlite::Row<'_>) -> Result<OrderNoteExpectedResponseInfoParsed, rusqlite::Error> {
    let serial_number: String = row.get(0)?;
    let receiver_account_id: String = row.get(1)?;
    let response_note_tag: Vec<u8> = row.get(2)?;
    let pool_id: String = row.get(3)?;
    Ok(
        OrderNoteExpectedResponseInfoParsed {
            serial_number,
            receiver_account_id,
            response_note_tag,
            pool_id,
        }
    )
}

fn parse_order_expected_response_result(parsed: OrderNoteExpectedResponseInfoParsed) -> Result<OrderNoteExpectedResponseInfo, StoreError> {
    Ok(OrderNoteExpectedResponseInfo {
        serial_number: Digest::try_from(&parsed.serial_number)?.into(),
        receiver_account_id: AccountId::from_hex(&parsed.receiver_account_id)?,
        response_note_tag: NoteTag::read_from_bytes(&parsed.response_note_tag)?,
        pool_id: AccountId::from_hex(&parsed.pool_id)?,
    })
}

fn parse_assets_column(row: &rusqlite::Row<'_>) -> Result<Vec<u8>, rusqlite::Error> {
    let assets: Vec<u8> = row.get(0)?;
    Ok(assets)
}

fn parse_assets(parsed: Result<Vec<u8>, rusqlite::Error>) -> Result<Option<NoteAssets>, StoreError> {
    match parsed {
        Ok(assets) => {
            NoteAssets::read_from_bytes(&assets)
                .map_err(|err| StoreError::DataDeserializationError(err)).map(|assets| Some(assets))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            Ok(None)
        }
        Err(err) => {
            Err(StoreError::DatabaseError(err.to_string()))
        }
    }
}