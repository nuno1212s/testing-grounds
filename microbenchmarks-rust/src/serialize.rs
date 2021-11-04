use std::default::Default;
use std::io::{Read, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use febft::bft::error::*;
use febft::bft::crypto::hash::Digest;
use febft::bft::communication::serialize::SharedData;
use febft::bft::communication::message::{
    Header,
    ReplyMessage,
    StoredMessage,
    SystemMessage,
    RequestMessage,
    ConsensusMessage,
    ConsensusMessageKind,
};
use febft::bft::collections::{
    self,
    HashMap,
};
use febft::bft::ordering::{
    SeqNo,
    Orderable,
};

pub struct MicrobenchmarkData;

impl SharedData for MicrobenchmarkData {
    type State = ();
    type Request = Vec<u8>;
    type Reply = ();

    fn serialize_state<W>(mut w: W, _s: &Self::State) -> Result<()>
    where
        W: Write
    {
        w.write_all(b"OK")
            .wrapped(ErrorKind::Communication)
    }

    fn deserialize_state<R>(_r: R) -> Result<Self::State>
    where
        R: Read
    {
        Ok(())
    }

    fn serialize_message<W>(mut w: W, m: &SystemMessage<(), Vec<u8>, ()>) -> Result<()>
    where
        W: Write
    {
        match m {
            SystemMessage::Request(m) => {
                // message kind
                w.write_i32::<BigEndian>(0x01).wrapped(ErrorKind::CommunicationSerialize)?;
                // request
                w.write_i32::<BigEndian>(m.operation().len() as i32).wrapped(ErrorKind::CommunicationSerialize)?;
                w.write_all(m.operation().as_ref()).wrapped(ErrorKind::CommunicationSerialize)?;
            },
            SystemMessage::Reply(m) => {
                // message kind
                w.write_i32::<BigEndian>(0x02).wrapped(ErrorKind::CommunicationSerialize)?;
            },
            SystemMessage::Consensus(m) => {
                match m.kind() {
                    ConsensusMessageKind::PrePrepare(requests) => {
                        // message kind
                        w.write_i32::<BigEndian>(0x03).wrapped(ErrorKind::CommunicationSerialize)?;
                        // number of requests
                        let num_reqs = requests.len();
                        w.write_i32::<BigEndian>(num_reqs as i32).wrapped(ErrorKind::CommunicationSerialize)?;
                        // length of each request
                        let req_len = requests[0].message().operation().len();
                        w.write_i32::<BigEndian>(req_len as i32).wrapped(ErrorKind::CommunicationSerialize)?;
                        // serialize requests
                        for stored in requests {
                            w.write_all(&{
                                let mut buf = [0; Header::LENGTH];
                                let _ = stored.header().serialize_into(&mut buf);
                                buf
                            }).wrapped(ErrorKind::CommunicationSerialize)?;

                            w.write_all(stored.message().operation())
                                .wrapped(ErrorKind::CommunicationSerialize)?;
                        }
                    },
                    ConsensusMessageKind::Prepare(digest) => {
                        // message kind
                        w.write_i32::<BigEndian>(0x04).wrapped(ErrorKind::CommunicationSerialize)?;
                        // digest
                        w.write_all(digest.as_ref()).wrapped(ErrorKind::CommunicationSerialize)?;
                    },
                    ConsensusMessageKind::Commit(digest) => {
                        // message kind
                        w.write_i32::<BigEndian>(0x05).wrapped(ErrorKind::CommunicationSerialize)?;
                        // digest
                        w.write_all(digest.as_ref()).wrapped(ErrorKind::CommunicationSerialize)?;
                    },
                }
            },
            _ => return Err("Unsupported system message").wrapped(ErrorKind::CommunicationSerialize),
        }
        Ok(())
    }

    fn deserialize_message<R>(mut r: R) -> Result<SystemMessage<(), Vec<u8>, ()>>
    where
        R: Read
    {
        let kind = r.read_i32::<BigEndian>().wrapped(ErrorKind::CommunicationSerialize)?;

        match kind {
            0x01 => {},
            0x02 => {},
            0x03 => {},
            0x04 => {},
            0x05 => {},
            _ => return Err("Unsupported system message").wrapped(ErrorKind::CommunicationSerialize),
        }

        unimplemented!()
    }
}
