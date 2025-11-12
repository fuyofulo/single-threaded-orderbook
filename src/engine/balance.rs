use std::collections::HashMap;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

pub struct AssetBalance {
    pub available: u64,
    pub locked: u64,
}

pub struct UserBalance {
    pub assets: HashMap<String, AssetBalance>,
}

pub struct Balances {
    pub users: HashMap<Uuid, UserBalance>,
}

impl Balances {
    pub fn new () -> Self {
        Self {
            users: HashMap::new()
        }
    }
}
