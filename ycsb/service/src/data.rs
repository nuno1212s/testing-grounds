use febft::bft::collections::HashMap;

#[derive(Clone)]
pub struct Request {
    pub table: String,
    pub key: String,
    pub values: HashMap<String, Vec<u8>>,
}

#[derive(Clone)]
pub struct Update {
    pub requests: Vec<Request>,
}
