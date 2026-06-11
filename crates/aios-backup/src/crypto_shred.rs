use crate::backup_set::BackupSet;
use crate::enums::BackupSetState;

/// Cryptographically destroy a backup set by transitioning it from Sealed to
/// Shredded. Only per-subject keys are destroyed; the evidence chain is
/// preserved.
///
/// # Errors
/// Returns `Err` if the set is not in the `Sealed` state.
pub fn crypto_shred_backup_set(set: &mut BackupSet) -> Result<BackupSetState, String> {
    if set.state != BackupSetState::Sealed {
        return Err("only sealed sets can be crypto-shredded".to_string());
    }
    set.state = BackupSetState::Shredded;
    Ok(set.state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup_set::BackupSet;

    #[test]
    fn shred_sealed_set_succeeds() {
        let mut set = BackupSet::new("cbc_01".into(), "host-1".into(), None);
        set.state = BackupSetState::Sealed;
        let result = crypto_shred_backup_set(&mut set);
        assert!(result.is_ok());
        assert_eq!(set.state, BackupSetState::Shredded);
    }

    #[test]
    fn shred_non_sealed_fails() {
        let mut set = BackupSet::new("cbc_01".into(), "host-1".into(), None);
        let result = crypto_shred_backup_set(&mut set);
        assert!(result.is_err());

        set.state = BackupSetState::Failed;
        let result = crypto_shred_backup_set(&mut set);
        assert!(result.is_err());

        set.state = BackupSetState::Expired;
        let result = crypto_shred_backup_set(&mut set);
        assert!(result.is_err());
    }

    #[test]
    fn shredded_set_is_terminal() {
        let mut set = BackupSet::new("cbc_01".into(), "host-1".into(), None);
        set.state = BackupSetState::Sealed;
        crypto_shred_backup_set(&mut set).expect("shred");
        assert!(set.is_terminal());
        assert!(!set.is_restorable());
    }
}
