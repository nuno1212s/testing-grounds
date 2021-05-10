use febft::bft::error::*;
use febft::bft::executable::{
    State,
    Service,
};
use febft::bft::collections;

use crate::serialize::YcsbData;
use crate::data::{Update, Request};

pub struct YcsbService;

impl Service for YcsbService {
    type Data = YcsbData;

    fn initial_state(&mut self) -> Result<State<Self>> {
        Ok(collections::hash_map())
    }

    fn update(&mut self, tables: &mut State<Self>, update: Update) -> u32 {
        for Request { table, key, values } in update.requests {
            let db = tables.entry(table).or_insert_with(collections::hash_map);
            db.insert(key, values);
        }
        0
    }
}
