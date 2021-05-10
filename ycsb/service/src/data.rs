use febft::bft::collections::HashMap;

pub struct Request {
    pub table: String,
    pub key: String,
    pub values: HashMap<String, Vec<u8>>,
}

pub struct Update {
    pub requests: Vec<Request>,
}
