use std::sync::Arc;

use febft::bft::error::*;
use febft::bft::executable::Service;

use crate::serialize::MicrobenchmarkData;

pub struct Microbenchmark {
    interval: usize,
    reply: Arc<Vec<u8>>,
}

impl Microbenchmark {
    pub fn new() -> Self {
        let reply = Arc::new((0..)
            .into_iter()
            .take(MicrobenchmarkData::REPLY_SIZE)
            .map(|x| (x & 0xff) as u8)
            .collect());

        Self {
            reply,
            interval: 0,
        }
    }
}

impl Service for Microbenchmark {
    type Data = MicrobenchmarkData;

    fn initial_state(&mut self) -> Result<Vec<u8>> {
        Ok((0..)
            .into_iter()
            .take(MicrobenchmarkData::STATE_SIZE)
            .map(|x| (x & 0xff) as u8)
            .collect())
    }

    fn update(&mut self, _state: &mut Vec<u8>, _request: Vec<u8>) -> Arc<Vec<u8>> {
        Arc::clone(&self.reply)
    }
}
