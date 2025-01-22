use miden_client::accounts::AccountStorageMode;
use clap::ValueEnum;



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