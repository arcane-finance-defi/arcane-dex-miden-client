use miden_objects::{accounts::AccountId, crypto::rand::FeltRng, notes::{ Note, NoteId, NoteRecipient, NoteTag, Nullifier }};

use crate::{Client, ClientError};

pub struct InsertOrderData {
    note_id: NoteId,
    note_nullifier: Nullifier,
    pool_id: AccountId,
    asset_in_faucet_id: AccountId,
    response_recipient: NoteRecipient,
    response_note_tag: NoteTag,
    
}

impl InsertOrderData {
    pub fn new(note_id: NoteId, note_nullifier: Nullifier, pool_id: AccountId, asset_in_faucet_id: AccountId, response_recipient: NoteRecipient, response_note_tag: NoteTag) -> Self {
        Self { note_id, note_nullifier, pool_id, asset_in_faucet_id, response_recipient, response_note_tag }
    }

    pub fn note_id(&self) -> &NoteId {
        &self.note_id
    }

    pub fn note_nullifier(&self) -> &Nullifier {
        &self.note_nullifier
    }

    pub fn pool_id(&self) -> &AccountId {
        &self.pool_id
    }

    pub fn asset_in_faucet_id(&self) -> &AccountId {
        &self.asset_in_faucet_id
    }

    pub fn response_recipient(&self) -> &NoteRecipient {
        &self.response_recipient
    }

    pub fn response_note_tag(&self) -> &NoteTag {
        &self.response_note_tag
    }
}

#[derive(Clone)]
pub struct OrderInfo {
    note_id: NoteId,
    pool_id: AccountId,
    receiver_account_id: AccountId,
    asset_in_faucet_id: AccountId,
    asset_amount: u64,
}

impl OrderInfo {
    pub fn new(note_id: NoteId, pool_id: AccountId, receiver_account_id: AccountId, asset_in_faucet_id: AccountId, asset_amount: u64) -> Self {
        Self { note_id, pool_id, receiver_account_id, asset_in_faucet_id, asset_amount }
    }

    pub fn note_id(&self) -> &NoteId {
        &self.note_id
    }

    pub fn pool_id(&self) -> &AccountId {
        &self.pool_id
    }

    pub fn receiver_account_id(&self) -> &AccountId {
        &self.receiver_account_id
    }

    pub fn asset_in_faucet_id(&self) -> &AccountId {
        &self.asset_in_faucet_id
    }

    pub fn asset_amount(&self) -> &u64 {
        &self.asset_amount
    }
}

impl<R: FeltRng> Client<R> {
    pub async fn insert_order(&mut self, order: InsertOrderData) -> Result<(), ClientError> {
        self.store.insert_order(order).await?;
        Ok(())
    }

    pub async fn find_order_result(&self, order_note_id: NoteId) -> Result<Option<Note>, ClientError> {
        self.store.find_order_result(order_note_id).await.map_err(ClientError::StoreError)
    }

    pub async fn get_order(&self, order_note_id: NoteId) -> Result<Option<OrderInfo>, ClientError> {
        self.store.get_order(order_note_id).await.map_err(ClientError::StoreError)
    }
}
