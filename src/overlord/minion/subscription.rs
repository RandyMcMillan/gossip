use crate::event_stream::EventStreamData;
use nostr_types::{ClientMessage, Filter, SubscriptionId};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Subscriptions {
    handle_to_id: HashMap<String, String>,
    by_id: HashMap<String, Subscription>,
}

impl Subscriptions {
    pub fn new() -> Subscriptions {
        Subscriptions {
            handle_to_id: HashMap::new(),
            by_id: HashMap::new(),
        }
    }

    pub fn add(&mut self, handle: &str, filters: Vec<Filter>, data: Option<Arc<EventStreamData>>) {
        let mut sub = Subscription::new();
        sub.filters = filters;
        sub.event_stream_data = data;
        self.handle_to_id.insert(handle.to_owned(), sub.get_id());
        self.by_id.insert(sub.get_id(), sub);
    }

    pub fn has(&self, handle: &str) -> bool {
        match self.handle_to_id.get(handle) {
            None => false,
            Some(id) => self.by_id.contains_key(id),
        }
    }

    pub fn get(&self, handle: &str) -> Option<Subscription> {
        match self.handle_to_id.get(handle) {
            None => None,
            Some(id) => self.by_id.get(id).cloned(),
        }
    }

    /*
    pub fn get_by_id(&self, id: &str) -> Option<Subscription> {
        self.by_id.get(id).cloned()
    }
     */

    pub fn get_handle_by_id(&self, id: &str) -> Option<String> {
        for (handle, xid) in self.handle_to_id.iter() {
            if id == xid {
                return Some(handle.to_string());
            }
        }
        None
    }

    pub fn get_mut(&mut self, handle: &str) -> Option<&mut Subscription> {
        match self.handle_to_id.get(handle) {
            None => None,
            Some(id) => self.by_id.get_mut(id),
        }
    }

    pub fn get_mut_by_id(&mut self, id: &str) -> Option<&mut Subscription> {
        self.by_id.get_mut(id)
    }

    pub fn remove(&mut self, handle: &str) {
        if let Some(id) = self.handle_to_id.get(handle) {
            self.by_id.remove(id);
            self.handle_to_id.remove(handle);
        }
    }

    /*
        pub fn remove_by_id(&mut self, id: &str) {
            self.by_id.remove(id);
    }
        */
}

#[derive(Clone, Debug)]
pub struct Subscription {
    id: String,
    filters: Vec<Filter>,
    eose: bool,
    pub event_stream_data: Option<Arc<EventStreamData>>
}

impl Subscription {
    pub fn new() -> Subscription {
        Subscription {
            id: textnonce::TextNonce::new().to_string(),
            filters: vec![],
            eose: false,
            event_stream_data: None,
        }
    }

    pub fn get_id(&self) -> String {
        self.id.clone()
    }

    pub fn get_mut(&mut self) -> &mut Vec<Filter> {
        &mut self.filters
    }

    pub fn set_eose(&mut self) {
        self.eose = true;
    }

    /*
    pub fn eose(&self) -> bool {
        self.eose
    }
     */

    pub fn req_message(&self) -> ClientMessage {
        ClientMessage::Req(SubscriptionId(self.get_id()), self.filters.clone())
    }

    pub fn close_message(&self) -> ClientMessage {
        ClientMessage::Close(SubscriptionId(self.get_id()))
    }
}
