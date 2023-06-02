use std::env;
use std::time::Duration;

use konst::primitive::{parse_bool, parse_u64, parse_usize};

pub fn id() -> usize {
    let string = env::var("ID").unwrap();
    
    parse_usize(&*string).unwrap()
}

pub fn request_size() -> usize {
    let string = env::var("REQUEST_SIZE").unwrap();

    parse_usize(&*string).unwrap()
}

pub fn reply_size() -> usize {
    let string = env::var("REPLY_SIZE").unwrap();

    parse_usize(&*string).unwrap()
}

pub fn state_size() -> usize {
    let string = env::var("STATE_SIZE").unwrap();

    parse_usize(&*string).unwrap()
}

pub fn measurement_interval() -> usize {
    let string = env::var("MEASUREMENT_INTERVAL").unwrap();

    parse_usize(&*string).unwrap()
}

pub fn ops_number() -> u64 {
    let string = env::var("OPS_NUMBER").unwrap();

    parse_u64(&*string).unwrap()
}

pub fn request_sleep_millis() -> Duration {
    let result = parse_u64(&*env::var("REQUEST_SLEEP_MILLIS")
        .unwrap_or(String::from("0"))).unwrap();

    Duration::from_millis(result)
}

pub fn verbose() -> bool {
    let request_sleep = env::var("REQUEST_SLEEP_MILLIS")
        .unwrap_or(String::from("0"));

    parse_bool(&*request_sleep).unwrap()
}

pub fn concurrent_requests() -> u64 {
    let concurrent_rqs = env::var("CONCURRENT_RQS").unwrap();

    parse_u64(&*concurrent_rqs).unwrap()
}

pub fn client_count() -> u64 {
    let clients = env::var("NUM_CLIENTS").unwrap();

    parse_u64(&*clients).unwrap()
}