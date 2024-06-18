use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use bip39::{Language, Mnemonic};

pub fn generate_mnemonic() -> anyhow::Result<Mnemonic> {
    Ok(Mnemonic::generate_in(Language::English, 12)?)
}

pub fn data_dir() -> PathBuf {
    let home = home::home_dir().expect("Could not find home directory");
    let default = home.join(".cashu_iced");

    default
}

pub fn save_seed(seed: &str) {
    fs::create_dir_all(data_dir()).expect("Could not create data dir");

    let path = data_dir().join("seed.txt");

    fs::write(path, seed).expect("Could not write seed");
}

pub fn get_seed() -> Option<Mnemonic> {
    let path = data_dir().join("seed.txt");
    let seed = fs::read_to_string(path).ok();

    seed.map(|s| Mnemonic::from_str(&s).ok()).flatten()
}
