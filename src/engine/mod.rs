use std::sync::mpsc::Receiver;
use uuid::Uuid;
use std::collections::HashMap;
use tokio::sync::oneshot;

pub mod balance;
pub mod orderbook;

use balance::{AssetBalance, UserBalance, Balances};
use orderbook::{OrderBook, Side, Order};

pub enum EngineCommand {
    InitializeUser { tx_oneshot: oneshot::Sender<String> },
    Deposit { user_id: Uuid, asset: String, amount: u64, tx_oneshot: oneshot::Sender<String> },
    GetBalances { user_id: Uuid, tx_oneshot: oneshot::Sender<Option<UserBalance>> },
    CreateOrder { user_id: Uuid, side: Side, price: u64, quantity: u64, tx_oneshot: oneshot::Sender<String> },
    CancelOrder { user_id: Uuid, order_id: Uuid, tx_oneshot: oneshot::Sender<String> }
}

pub fn run(rx: Receiver<EngineCommand>) {
    println!("engine thread has started...");

    let mut balances = Balances::new();
    let mut orderbook = OrderBook::new();
    let mut order_index: HashMap<Uuid, (Side, u64)> = HashMap::new();

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
                        entry.available = entry.available
                            .checked_add(amount)
                            .expect("overflow in deposit");

                        let _ = tx_oneshot.send(format!("deposited {} {} for user {}", amount, asset, user_id));
                    } else {
                        let _ = tx_oneshot.send(format!("unknown asset!"));
                    }
                } else {
                    let _ = tx_oneshot.send(format!("user id: {} not found", user_id));
                }
            }
            EngineCommand::GetBalances {user_id, tx_oneshot} => {
                println!("fetching user balances");
                
                if let Some(user_balance) = balances.users.get(&user_id) {
                    let _ = tx_oneshot.send(Some(user_balance.clone()));
                } else {
                    let _ = tx_oneshot.send(None);
                }
            }
            EngineCommand::CreateOrder { user_id, side, price, quantity, tx_oneshot } => {
                let user = if let Some(u) = balances.users.get_mut(&user_id) {
                    u
                } else {
                    let _ = tx_oneshot.send("user not found".into());
                    continue;
                };
            
                match side {
                    Side::Bid => {
                        let cost_micro = match calculate_cost_usdc_micro(price, quantity) {
                            Some(c) => c,
                            None => {
                                let _ = tx_oneshot.send("cost overflow - invalid order".into());
                                continue;
                            }
                        };
                        let usdc = user.assets.get_mut("USDC").unwrap();
                        if usdc.available < cost_micro {
                            let _ = tx_oneshot.send("insufficient USDC funds".into());
                            continue;
                        }
                        usdc.available -= cost_micro;
                        usdc.locked += cost_micro;
                    }
                    Side::Ask => {
                        let btc = user.assets.get_mut("BTC").unwrap();
                        if btc.available < quantity {
                            let _ = tx_oneshot.send("insufficient BTC funds".into());
                            continue;
                        }
                        btc.available -= quantity;
                        btc.locked += quantity;
                    }
                }
            
                if let Side::Bid = side {
                    let mut remaining_qty = quantity;
            
                    loop {
                        let best_ask_entry = {
                            let mut it = orderbook.asks.iter_mut();
                            it.next()
                        };
            
                        let (best_price, queue) = match best_ask_entry {
                            Some((p, q)) => (*p, q),
                            None => break,
                        };
            
                        if price < best_price {
                            break;
                        }
            
                        let resting = queue.front_mut().unwrap();
                        let trade_qty = remaining_qty.min(resting.quantity);
                        let trade_cost = calculate_cost_usdc_micro(best_price, trade_qty).unwrap();
            
                        let maker = resting.user_id;
                        let taker = user_id;
            
                        {
                            let buyer = balances.users.get_mut(&taker).unwrap();
                            let btc = buyer.assets.get_mut("BTC").unwrap();
                            let usdc = buyer.assets.get_mut("USDC").unwrap();
                            btc.available += trade_qty;
                            usdc.locked -= trade_cost;
                        }
            
                        {
                            let seller = balances.users.get_mut(&maker).unwrap();
                            let btc = seller.assets.get_mut("BTC").unwrap();
                            let usdc = seller.assets.get_mut("USDC").unwrap();
                            btc.locked -= trade_qty;
                            usdc.available += trade_cost;
                        }
            
                        resting.quantity -= trade_qty;
                        remaining_qty -= trade_qty;
            
                        if resting.quantity == 0 {
                            let finished = queue.pop_front().unwrap();
                            order_index.remove(&finished.id);
                            if queue.is_empty() {
                                orderbook.asks.remove(&best_price);
                            }
                        }
            
                        if remaining_qty == 0 {
                            break;
                        }
                    }
            
                    if remaining_qty > 0 {
                        let order_id = Uuid::new_v4();
                        order_index.insert(order_id, (Side::Bid, price));
            
                        let new_order = Order {
                            id: order_id,
                            user_id,
                            side: Side::Bid,
                            price,
                            quantity: remaining_qty,
                        };
            
                        orderbook.add_order(new_order);
                        let _ = tx_oneshot.send(order_id.to_string());
                    } else {
                        let _ = tx_oneshot.send("order fully filled".into());
                    }
            
                    continue;
                }
            
                if let Side::Ask = side {
                    let mut remaining_qty = quantity;
            
                    loop {
                        let best_bid_entry = {
                            let mut it = orderbook.bids.iter_mut().rev();
                            it.next()
                        };
            
                        let (best_price, queue) = match best_bid_entry {
                            Some((p, q)) => (*p, q),
                            None => break,
                        };
            
                        if price > best_price {
                            break;
                        }
            
                        let resting = queue.front_mut().unwrap();
                        let trade_qty = remaining_qty.min(resting.quantity);
                        let trade_cost = calculate_cost_usdc_micro(best_price, trade_qty).unwrap();
            
                        let maker = resting.user_id;
                        let taker = user_id;
            
                        {
                            let seller = balances.users.get_mut(&taker).unwrap();
                            let btc = seller.assets.get_mut("BTC").unwrap();
                            let usdc = seller.assets.get_mut("USDC").unwrap();
                            btc.locked -= trade_qty;
                            usdc.available += trade_cost;
                        }
            
                        {
                            let buyer = balances.users.get_mut(&maker).unwrap();
                            let btc = buyer.assets.get_mut("BTC").unwrap();
                            let usdc = buyer.assets.get_mut("USDC").unwrap();
                            btc.available += trade_qty;
                            usdc.locked -= trade_cost;
                        }
            
                        resting.quantity -= trade_qty;
                        remaining_qty -= trade_qty;
            
                        if resting.quantity == 0 {
                            let finished = queue.pop_front().unwrap();
                            order_index.remove(&finished.id);
                            if queue.is_empty() {
                                orderbook.bids.remove(&best_price);
                            }
                        }
            
                        if remaining_qty == 0 {
                            break;
                        }
                    }
            
                    if remaining_qty > 0 {
                        let order_id = Uuid::new_v4();
                        order_index.insert(order_id, (Side::Ask, price));
            
                        let new_order = Order {
                            id: order_id,
                            user_id,
                            side: Side::Ask,
                            price,
                            quantity: remaining_qty,
                        };
            
                        orderbook.add_order(new_order);
                        let _ = tx_oneshot.send(order_id.to_string());
                    } else {
                        let _ = tx_oneshot.send("order fully filled".into());
                    }
            
                    continue;
                }
            }            
            EngineCommand::CancelOrder {user_id, order_id, tx_oneshot} => {
                let (side, price) = match order_index.remove(&order_id) {
                    Some(v) => v,
                    None => {
                        let _ = tx_oneshot.send("order not found".into());
                        continue;
                    }
                };

                let side = match side {
                    Side::Ask => &mut orderbook.asks,
                    Side::Bid => &mut orderbook.bids,
                };

                let queue = match side.get_mut(&price) {
                    Some(q) => q,
                    None => {
                        let _ = tx_oneshot.send("order missing from orderbook".into());
                        continue;
                    }
                };

                let pos = queue.iter().position(|o| o.id == order_id);

                let removed_order = match pos {
                    Some(p) => queue.remove(p).unwrap(),
                    None => {
                        let _ = tx_oneshot.send("order not found in price level".into());
                        continue;
                    }
                };

                if queue.is_empty() {
                    side.remove(&price);
                }

                let user = balances.users.get_mut(&user_id).unwrap();

                match removed_order.side {
                    Side::Bid => {
                        let cost_micro = calculate_cost_usdc_micro(price, removed_order.quantity).unwrap();
                        let usdc = user.assets.get_mut("USDC").unwrap();
                        usdc.locked -= cost_micro;
                        usdc.available += cost_micro;
                    }
                    Side::Ask => {
                        let btc = user.assets.get_mut("BTC").unwrap();
                        btc.locked -= removed_order.quantity;
                        btc.available += removed_order.quantity;
                    }
                }

                let _ = tx_oneshot.send(format!("order:{} has been cancelled!", order_id));
            }
        }
    }

}

fn calculate_cost_usdc_micro(price_micro: u64, qty_sats: u64) -> Option<u64> {
    let p = price_micro as u128;
    let q = qty_sats as u128;

    let total = p.checked_mul(q)?;
    let cost = total.checked_div(100_000_000u128)?;

    u64::try_from(cost).ok()
}
