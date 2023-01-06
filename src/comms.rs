use crate::event_stream::EventStreamData;
use nostr_types::{Event, Filter, Id, IdHex, PublicKey, PublicKeyHex};
use std::sync::Arc;

/// This is a message sent to the Overlord
#[derive(Debug, Clone)]
pub enum ToOverlordMessage {
    AddRelay(String),
    DeletePub,
    FollowBech32(String, String),
    FollowHex(String, String),
    FollowNip35(String),
    GeneratePrivateKey(String),
    GetMissingEvents,
    ImportPriv(String, String),
    ImportPub(String),
    Like(Id, PublicKey),
    MinionIsReady,
    ProcessIncomingEvents,
    PostReply(String, Id),
    PostTextNote(String),
    SaveRelays,
    SaveSettings,
    Shutdown,
    UnlockKey(String),
    UpdateMetadata(PublicKeyHex),
}

/// This is a message sent to the minions
#[derive(Debug, Clone)]
pub struct ToMinionMessage {
    /// The minion we are addressing, based on the URL they are listening to
    /// as a String.  "all" means all minions.
    pub target: String,

    pub payload: ToMinionPayload,
}

#[derive(Debug, Clone)]
pub enum ToMinionPayload {
    Shutdown,
    SubscribeEventStream(Arc<EventStreamData>, Vec<Filter>),
    SubscribeGeneralFeed,
    SubscribePersonFeed(PublicKeyHex),
    SubscribeThreadFeed(Id),
    TempSubscribeMetadata(PublicKeyHex),
    FetchEvents(Vec<IdHex>),
    PostEvent(Box<Event>),
}
