use std::sync::mpsc::Receiver;
use uuid::Uuid;
use std::collections::HashMap;
use tokio::sync::oneshot;

pub mod balance;
pub mod orderbook;

use balance::{AssetBalance, UserBalance, Balances};

pub enum EngineCommand {
    InitializeUser { tx_oneshot: oneshot::Sender<String> },
    Deposit { user_id: Uuid, asset: String, amount: u64, tx_oneshot: oneshot::Sender<String> },
    GetBalances { user_id: Uuid, tx_oneshot: oneshot::Sender<String> },
    CreateOrder { tx_oneshot: oneshot::Sender<String> },
    CancelOrder { tx_oneshot: oneshot::Sender<String> }
}

pub fn run(rx: Receiver<EngineCommand>) {
    println!("engine thread has started...");

    let mut balances = Balances::new();

    for cmd in rx {
        match cmd {
            EngineCommand::InitializeUser { tx_oneshot} => {
                let user_id = Uuid::new_v4();
                println!("initializing balances for {user_id}");
                balances.users.insert(
                    user_id,
                    UserBalance {
                        assets: HashMap::from([
                            ("BTC".to_string(), AssetBalance { available: 0, locked: 0 } ),
                            ("USDC".to_string(), AssetBalance { available: 0, locked: 0 } ),
                        ])
                });

                let _ = tx_oneshot.send(user_id.to_string());
            }
            EngineCommand::Deposit {user_id, asset, amount, tx_oneshot} => {
                if let Some(user) = balances.users.get_mut(&user_id) {
                    if let Some(entry) = user.assets.get_mut(&asset) {
                        entry.available = entry.avilable.saturating_add(amount);
                        let _ = tx_oneshot.send(format!("deposited {} {} for user {}", amount, asset, user_id));
                    } else {
                        let _ = tx_oneshot.send(format!("unknown asset!"));
                    }
                } else {
                    let _ = tx_oneshot.send(format!("user id: {} not found"));
                }
            }
            EngineCommand::GetBalances {tx_oneshot} => {
                println!("fetching user balances");
                
                if let Some(user_balance) = balances.users.get(&user_id) {
                    let serialized = serde_json::to_string(&user_balance).unwrap();
                    let _ = tx_oneshot.send(Some(serialized));
                } else {
                    let _ = tx_oneshot.send("user not found".into());
                }
            }
            EngineCommand::CreateOrder {tx_oneshot} => {
                println!("creating order for user");
                let _ = tx_oneshot.send("created order for user".into());
            }
            EngineCommand::CancelOrder {tx_oneshot} => {
                println!("cancelling order for user");
                let _ = tx_oneshot.send("cancelled order for user".into());
            }
        }
    }

}
