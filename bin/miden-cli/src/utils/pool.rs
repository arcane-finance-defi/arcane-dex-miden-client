use miden_client::{accounts::{Account, AccountId}, assets::FungibleAsset, crypto::Digest, ClientError, Word};

pub fn pool_account_code_commitment() -> Digest {
    Digest::try_from("0xd98b15d43e3ecb7980bf577a6877c54fec756576ea58769ddf1f4c5287440e79").unwrap()
}

const POOL_PAIR_STORAGE_INDEX: u8 = 0;

pub struct Pool {
    account: Account
}

pub fn is_pool_account(account: &Account) -> Result<bool, ClientError> {
    return Ok(account.code().commitment() == pool_account_code_commitment());
}

impl From<Account> for Pool {
    fn from(account: Account) -> Self {
        Pool { account }
    }
}

pub fn calculate_amount_out(reserve_in: u64, reserve_out: u64, amount_in: u64) -> u64 {
    let scaled_reserve_in: u64 = (reserve_in * 1000) as u64;
    let scaled_reserve_out: u64 = (reserve_out * 1000) as u64;
    let scaled_amount_in: u64 = (amount_in * 1000) as u64;

    let numerator = (scaled_amount_in * scaled_reserve_out) / 1000000;
    let denominator = (scaled_reserve_in + scaled_amount_in) / 1000;
    numerator / denominator
}

impl Pool {

    pub fn account(&self) -> &Account {
        &self.account
    }

    pub fn get_pool_supported_assets(&self) -> Result<[AccountId; 2], ClientError> {
        let pool_pair: Word = self.account.storage()
            .get_item(POOL_PAIR_STORAGE_INDEX)
            .map_err(|_| ClientError::PoolPairNotFoundInAccountStorage(self.account.id()))?
            .into();

        Ok([
            AccountId::try_from([pool_pair[0], pool_pair[1]])?,
            AccountId::try_from([pool_pair[2], pool_pair[3]])?
        ])
    }

    pub fn is_asset_supported(&self, asset: AccountId) -> Result<bool, ClientError> {
        let pool_supported_assets = self.get_pool_supported_assets()?;

        Ok(pool_supported_assets.contains(&asset))
    }

    pub fn calculate_asset_out(&self, asset_in: FungibleAsset) -> Result<FungibleAsset, ClientError> {
        let asset_out_faucet_id = self.get_pool_supported_assets()?.into_iter()
            .find(|asset_id| asset_id.clone().ne(&asset_in.faucet_id())).unwrap();

        let reserve_in = self.account.vault().get_balance(asset_in.faucet_id().clone()).unwrap_or(0);
        let reserve_out = self.account.vault().get_balance(asset_out_faucet_id.clone()).unwrap_or(0);
        let amount_in = asset_in.amount();

        let amount_out = calculate_amount_out(reserve_in, reserve_out, amount_in);
        Ok(FungibleAsset::new(asset_out_faucet_id.clone(), amount_out).unwrap())
    }
}