use febft::bft::error::*;
use febft::bft::executable::Service;

use crate::serialize::MicrobenchmarkData;

pub struct Microbenchmark;

impl Service for Microbenchmark {
    type Data = MicrobenchmarkData;

    fn initial_state(&mut self) -> Result<()> {
        Ok(())
    }

    fn update(&mut self, _state: &mut (), _request: Vec<u8>) {
        // nothing
    }
}
