use febft::bft::error::*;
use febft::bft::collections;
use febft::bft::executable::Service;

use crate::data::Update;
use crate::serialize::{
    YcsbData,
    YcsbDataState,
};

pub struct YcsbService;

impl Service for YcsbService {
    type Data = YcsbData;

    fn initial_state(&mut self) -> Result<YcsbDataState> {
        Ok(collections::hash_map())
    }

    fn update(
        &mut self,
        tables: &mut YcsbDataState,
        Update { table, key, values }: Update,
    ) -> u32 {
        let db = tables.entry(table).or_insert_with(collections::hash_map);
        db.insert(key, values);
        0
    }
}
