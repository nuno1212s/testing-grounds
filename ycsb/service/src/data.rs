use febft::bft::collections::HashMap;

#[derive(Clone)]
pub struct Update {
    pub table: String,
    pub key: String,
    pub values: HashMap<String, Vec<u8>>,
}
