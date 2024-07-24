use std::sync::Arc;
use atlas_communication::message::Header;
use atlas_communication::message_signing::NetworkMessageSignatureVerifier;
use atlas_communication::reconfiguration_node::NetworkInformationProvider;
use atlas_communication::serialize::Serializable;

pub(crate) struct ReconfigurableMessage {}

impl Serializable for ReconfigurableMessage {
    type Message = Arc<[u8]>;

    fn verify_message_internal<NI, SV>(info_provider: &Arc<NI>, header: &Header, msg: &Self::Message) -> atlas_common::error::Result<()>
    where
        NI: NetworkInformationProvider + 'static,
        SV: NetworkMessageSignatureVerifier<Self, NI>,
        Self: Sized
    {
        Ok(())
    }

}

pub(crate) struct ProtocolMessage {}

impl Serializable for ProtocolMessage {
    type Message = Arc<[u8]>;

    fn verify_message_internal<NI, SV>(info_provider: &Arc<NI>, header: &Header, msg: &Self::Message) -> atlas_common::error::Result<()>
    where
        NI: NetworkInformationProvider + 'static,
        SV: NetworkMessageSignatureVerifier<Self, NI>,
        Self: Sized
    {
        Ok(())
    }

}