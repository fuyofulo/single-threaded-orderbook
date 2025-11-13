use std::collections::{BTreeMap, VecDeque};
use uuid::Uuid;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Side {
    Ask, 
    Bid
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Order {
    pub id: Uuid,
    pub user_id: Uuid,
    pub side: Side,
    pub price: u64,
    pub quantity: u64
}

#[derive(Debug, Clone)]
pub struct OrderBook {
    pub bids: BTreeMap<u64, VecDeque<Order>>,
    pub asks: BTreeMap<u64, VecDeque<Order>>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    pub fn add_order(&mut self, order: Order) {

        let book_side = match order.side {
            Side::Ask => &mut self.asks,
            Side::Bid => &mut self.bids,
        };

        book_side
            .entry(order.price)
            .or_insert_with(VecDeque::new)
            .push_back(order);
    }
}
