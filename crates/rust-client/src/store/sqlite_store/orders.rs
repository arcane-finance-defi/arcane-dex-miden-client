
use alloc::{
    rc::Rc,
    string::ToString,
    vec::Vec,
};

use dex_poc::notes::build_swap_result_from_parts;
use miden_objects::{
    accounts::AccountId, crypto::utils::{Deserializable, Serializable}, notes::{Note, NoteAssets, NoteId, NoteTag}, Digest, Word
};

use rusqlite::{params, types::Value, Connection};

use crate::{order::InsertOrderData, store::{NoteRecordError, StoreError}, sync::NoteTagSource};

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

        let order_result = parse_order_result(tx.prepare(FIND_ORDER_QUERY)?
            .query_row(params![order_note_id.to_string()], parse_order_columns)
            .expect(&format!("Order with id {:?} not found", order_note_id))
        )?;

        const FIND_OUTPUT_NOTE_ASSETS_QUERY: &str = "SELECT assets FROM output_notes WHERE tag = ?";

        let assets = parse_assets(
            tx.query_row(FIND_OUTPUT_NOTE_ASSETS_QUERY, params![order_result.response_note_tag.to_bytes()], parse_assets_column)?
        )?;

        tx.commit()?;

        return build_swap_result_from_parts(
            order_result.receiver_account_id, 
            order_result.serial_number, 
            assets, 
            order_result.pool_id, 
            order_result.response_note_tag
        ).map(|note| Some(note))
        .map_err(|err| NoteRecordError::NoteError(err).into());

    }
    
}

struct OrderNoteExpectedResponseInfoParsed {
    serial_number: Vec<u8>,
    receiver_account_id: Vec<u8>,
    response_note_tag: Vec<u8>,
    pool_id: Vec<u8>,
}

struct OrderNoteExpectedResponseInfo {
    serial_number: Word,
    receiver_account_id: AccountId,
    response_note_tag: NoteTag, 
    pool_id: AccountId,
}

fn parse_order_columns(row: &rusqlite::Row<'_>) -> Result<OrderNoteExpectedResponseInfoParsed, rusqlite::Error> {
    let serial_number: Vec<u8> = row.get(0)?;
    let receiver_account_id: Vec<u8> = row.get(1)?;
    let response_note_tag: Vec<u8> = row.get(2)?;
    let pool_id: Vec<u8> = row.get(3)?;
    Ok(
        OrderNoteExpectedResponseInfoParsed {
            serial_number,
            receiver_account_id,
            response_note_tag,
            pool_id,
        }
    )
}

fn parse_order_result(parsed: OrderNoteExpectedResponseInfoParsed) -> Result<OrderNoteExpectedResponseInfo, StoreError> {
    Ok(OrderNoteExpectedResponseInfo {
        serial_number: Word::read_from_bytes(&parsed.serial_number)?,
        receiver_account_id: AccountId::read_from_bytes(&parsed.receiver_account_id)?,
        response_note_tag: NoteTag::read_from_bytes(&parsed.response_note_tag)?,
        pool_id: AccountId::read_from_bytes(&parsed.pool_id)?,
    })
}

fn parse_assets_column(row: &rusqlite::Row<'_>) -> Result<Vec<u8>, rusqlite::Error> {
    let assets: Vec<u8> = row.get(0)?;
    Ok(assets)
}

fn parse_assets(parsed: Vec<u8>) -> Result<NoteAssets, StoreError> {
    Ok(NoteAssets::read_from_bytes(&parsed)?)
}