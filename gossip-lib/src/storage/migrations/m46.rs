use crate::error::Error;
use crate::storage::Storage;
use heed::RwTxn;

impl Storage {
    pub(super) fn m46_trigger(&self) -> Result<(), Error> {
        Ok(())
    }

    pub(super) fn m46_migrate<'a>(
        &'a self,
        prefix: &str,
        txn: &mut RwTxn<'a>,
    ) -> Result<(), Error> {
        // Info message
        tracing::info!("{prefix}: Flagging that tag index need to be rebuilt...");

        // Rebuild relationships
        self.set_flag_rebuild_tag_index_needed(true, Some(txn))?;

        Ok(())
    }
}
