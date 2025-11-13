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
    CancelOrder { user_id: Uuid, order_id: Uuid, tx_oneshot: oneshot::Sender<String> },
    GetUserOrders { user_id: Uuid, tx_oneshot: oneshot::Sender<Vec<Order>> }
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
            
                        let (buyer_btc, buyer_usdc) = {
                            let assets = &mut user.assets;
                            let btc_ptr = assets.get_mut("BTC").unwrap() as *mut AssetBalance;
                            let usdc_ptr = assets.get_mut("USDC").unwrap() as *mut AssetBalance;
                            unsafe { (&mut *btc_ptr, &mut *usdc_ptr) }
                        };
            
                        if buyer_usdc.available < cost_micro {
                            let _ = tx_oneshot.send("insufficient USDC funds".into());
                            continue;
                        }
            
                        buyer_usdc.available -= cost_micro;
                        buyer_usdc.locked += cost_micro;
            
                        let mut remaining = quantity;
            
                        let ask_prices: Vec<u64> = orderbook
                            .asks
                            .keys()
                            .cloned()
                            .filter(|p| *p <= price)
                            .collect();
            
                        for ask_price in ask_prices {
                            let queue = orderbook.asks.get_mut(&ask_price).unwrap();
            
                            while let Some(mut ask_order) = queue.pop_front() {
                                let seller = balances.users.get_mut(&ask_order.user_id).unwrap();
            
                                let (seller_btc, seller_usdc) = {
                                    let assets = &mut seller.assets;
                                    let btc_ptr = assets.get_mut("BTC").unwrap() as *mut AssetBalance;
                                    let usdc_ptr = assets.get_mut("USDC").unwrap() as *mut AssetBalance;
                                    unsafe { (&mut *btc_ptr, &mut *usdc_ptr) }
                                };
            
                                let trade_qty = remaining.min(ask_order.quantity);
                                let trade_cost = calculate_cost_usdc_micro(ask_price, trade_qty).unwrap();
            
                                buyer_btc.available += trade_qty;
                                buyer_usdc.locked -= trade_cost;
            
                                seller_btc.locked -= trade_qty;
                                seller_usdc.available += trade_cost;
            
                                ask_order.quantity -= trade_qty;
                                remaining -= trade_qty;
            
                                if ask_order.quantity > 0 {
                                    queue.push_front(ask_order);
                                    break;
                                }
            
                                if remaining == 0 {
                                    break;
                                }
                            }
            
                            if queue.is_empty() {
                                orderbook.asks.remove(&ask_price);
                            }
            
                            if remaining == 0 {
                                break;
                            }
                        }
            
                        if remaining > 0 {
                            let order_id = Uuid::new_v4();
                            order_index.insert(order_id, (Side::Bid, price));
            
                            let resting_order = Order {
                                id: order_id,
                                user_id,
                                side: Side::Bid,
                                price,
                                quantity: remaining,
                            };
            
                            orderbook.add_order(resting_order);
                            let _ = tx_oneshot.send(order_id.to_string());
                        } else {
                            let _ = tx_oneshot.send("filled".into());
                        }
                    }
            
                    Side::Ask => {
                        let (seller_btc, seller_usdc) = {
                            let assets = &mut user.assets;
                            let btc_ptr = assets.get_mut("BTC").unwrap() as *mut AssetBalance;
                            let usdc_ptr = assets.get_mut("USDC").unwrap() as *mut AssetBalance;
                            unsafe { (&mut *btc_ptr, &mut *usdc_ptr) }
                        };
            
                        if seller_btc.available < quantity {
                            let _ = tx_oneshot.send("insufficient BTC funds".into());
                            continue;
                        }
            
                        seller_btc.available -= quantity;
                        seller_btc.locked += quantity;
            
                        let mut remaining = quantity;
            
                        let bid_prices: Vec<u64> = orderbook
                            .bids
                            .keys()
                            .cloned()
                            .filter(|p| *p >= price)
                            .collect();
            
                        for bid_price in bid_prices {
                            let queue = orderbook.bids.get_mut(&bid_price).unwrap();
            
                            while let Some(mut bid_order) = queue.pop_front() {
                                let buyer = balances.users.get_mut(&bid_order.user_id).unwrap();
            
                                let (buyer_btc, buyer_usdc) = {
                                    let assets = &mut buyer.assets;
                                    let btc_ptr = assets.get_mut("BTC").unwrap() as *mut AssetBalance;
                                    let usdc_ptr = assets.get_mut("USDC").unwrap() as *mut AssetBalance;
                                    unsafe { (&mut *btc_ptr, &mut *usdc_ptr) }
                                };
            
                                let trade_qty = remaining.min(bid_order.quantity);
                                let trade_cost = calculate_cost_usdc_micro(bid_price, trade_qty).unwrap();
            
                                seller_btc.locked -= trade_qty;
                                seller_usdc.available += trade_cost;
            
                                buyer_btc.available += trade_qty;
                                buyer_usdc.locked -= trade_cost;
            
                                bid_order.quantity -= trade_qty;
                                remaining -= trade_qty;
            
                                if bid_order.quantity > 0 {
                                    queue.push_front(bid_order);
                                    break;
                                }
            
                                if remaining == 0 {
                                    break;
                                }
                            }
            
                            if queue.is_empty() {
                                orderbook.bids.remove(&bid_price);
                            }
            
                            if remaining == 0 {
                                break;
                            }
                        }
            
                        if remaining > 0 {
                            let order_id = Uuid::new_v4();
                            order_index.insert(order_id, (Side::Ask, price));
            
                            let resting_order = Order {
                                id: order_id,
                                user_id,
                                side: Side::Ask,
                                price,
                                quantity: remaining,
                            };
            
                            orderbook.add_order(resting_order);
                            let _ = tx_oneshot.send(order_id.to_string());
                        } else {
                            let _ = tx_oneshot.send("filled".into());
                        }
                    }
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
            EngineCommand::GetUserOrders { user_id, tx_oneshot } => {
                let mut user_orders = Vec::new();
                for (_price, queue) in &orderbook.bids {
                    for order in queue {
                        if order.user_id == user_id {
                            user_orders.push(order.clone());
                        }
                    }
                }
                for (_price, queue) in &orderbook.asks {
                    for order in queue {
                        if order.user_id == user_id {
                            user_orders.push(order.clone());
                        }
                    }
                }
                let _ = tx_oneshot.send(user_orders);
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
