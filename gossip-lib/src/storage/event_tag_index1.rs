use crate::error::{Error, ErrorKind};
use crate::globals::GLOBALS;
use crate::storage::{RawDatabase, Storage};
use heed::{types::UnalignedSlice, DatabaseFlags, RwTxn};
use nostr_types::{Event, EventKind, PublicKeyHex};
use std::sync::Mutex;

// NOTE: "innerp" is a fake tag. We store events that reference a person internally under it.
pub(super) const INDEXED_TAGS: [&str; 4] = ["a", "d", "p", "delegation"];

// TagKey:QUOTE:TagValue -> Id
// (dup keys, so multiple Ids per key)
//   val: id.as_slice() | Id(val[0..32].try_into()?)

static EVENT_TAG_INDEX1_DB_CREATE_LOCK: Mutex<()> = Mutex::new(());
static mut EVENT_TAG_INDEX1_DB: Option<RawDatabase> = None;

impl Storage {
    pub(super) fn db_event_tag_index1(&self) -> Result<RawDatabase, Error> {
        unsafe {
            if let Some(db) = EVENT_TAG_INDEX1_DB {
                Ok(db)
            } else {
                // Lock.  This drops when anything returns.
                let _lock = EVENT_TAG_INDEX1_DB_CREATE_LOCK.lock();

                // In case of a race, check again
                if let Some(db) = EVENT_TAG_INDEX1_DB {
                    return Ok(db);
                }

                // Create it. We know that nobody else is doing this and that
                // it cannot happen twice.
                let mut txn = self.env.write_txn()?;
                let db = self
                    .env
                    .database_options()
                    .types::<UnalignedSlice<u8>, UnalignedSlice<u8>>()
                    .flags(DatabaseFlags::DUP_SORT | DatabaseFlags::DUP_FIXED)
                    .name("event_tag_index")
                    .create(&mut txn)?;
                txn.commit()?;
                EVENT_TAG_INDEX1_DB = Some(db);
                Ok(db)
            }
        }
    }

    pub fn write_event_tag_index1<'a>(
        &'a self,
        event: &Event,
        rw_txn: Option<&mut RwTxn<'a>>,
    ) -> Result<(), Error> {
        let f = |txn: &mut RwTxn<'a>| -> Result<(), Error> {
            let mut event = event;

            let mut rumor_event: Event;
            if event.kind == EventKind::GiftWrap {
                match GLOBALS.signer.unwrap_giftwrap(event) {
                    Ok(rumor) => {
                        rumor_event = rumor.into_event_with_bad_signature();
                        rumor_event.id = event.id; // lie, so it indexes it under the giftwrap
                        event = &rumor_event;
                    }
                    Err(e) => {
                        if matches!(e.kind, ErrorKind::NoPrivateKey) {
                            // Store as unindexed for later indexing
                            let bytes = vec![];
                            self.db_unindexed_giftwraps()?
                                .put(txn, event.id.as_slice(), &bytes)?;
                        }
                    }
                }
            }

            // our user's public key
            let pk: Option<PublicKeyHex> = self.read_setting_public_key().map(|p| p.into());

            for tag in &event.tags {
                let tagname = tag.tagname();
                let value = match tag.value(1) {
                    Ok(v) => v,
                    Err(_) => continue, // no tag value, not indexable.
                };

                // Only index tags we intend to lookup later by tag.
                // If that set changes, (1) add to this code and (2) do a reindex migration
                if !INDEXED_TAGS.contains(&&*tagname) {
                    continue;
                }
                // For 'p' tags, only index them if 'p' is our user
                if tagname == "p" {
                    match &pk {
                        None => continue,
                        Some(pk) => {
                            if value != pk.as_str() {
                                continue;
                            }
                        }
                    }
                }

                let mut key: Vec<u8> = tagname.as_bytes().to_owned();
                key.push(b'\"'); // double quote separator, unlikely to be inside of a tagname
                key.extend(value.as_bytes());
                let key = key!(&key); // limit the size
                let bytes = event.id.as_slice();
                self.db_event_tag_index()?.put(txn, key, bytes)?;
            }

            Ok(())
        };

        match rw_txn {
            Some(txn) => f(txn)?,
            None => {
                let mut txn = self.env.write_txn()?;
                f(&mut txn)?;
                txn.commit()?;
            }
        };

        Ok(())
    }
}
