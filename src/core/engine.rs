//! The reversible-action engine: apply an [`Action`], get a [`Reversal`]; feed a
//! [`Reversal`] back to undo it. This is the single primitive that rollback,
//! modes, and config-replay are all built on.

use std::io;

use crate::core::action::{Action, Reversal};
use crate::core::registry;

/// Apply an action and return the [`Reversal`] that undoes it.
pub fn apply(action: &Action) -> io::Result<Reversal> {
    match action {
        Action::SetReg { hive, path, name, value } => {
            let previous = registry::read(*hive, path, name)?;
            registry::write(*hive, path, name, value)?;
            Ok(Reversal::RestoreReg {
                hive: *hive,
                path: path.clone(),
                name: name.clone(),
                previous,
            })
        }
        Action::DeleteReg { hive, path, name } => {
            let previous = registry::read(*hive, path, name)?;
            registry::delete(*hive, path, name)?;
            Ok(Reversal::RestoreReg {
                hive: *hive,
                path: path.clone(),
                name: name.clone(),
                previous,
            })
        }
    }
}

/// Undo a previously-applied action.
pub fn revert(reversal: &Reversal) -> io::Result<()> {
    match reversal {
        Reversal::RestoreReg { hive, path, name, previous } => match previous {
            Some(v) => registry::write(*hive, path, name, v),
            None => registry::delete(*hive, path, name),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::action::{Hive, RegValue};
    use crate::core::registry;

    const TEST_PATH: &str = "Software\\NeonPrime\\Test";

    #[test]
    fn set_new_value_then_revert_removes_it() {
        let name = "EngineSetNew";
        let _ = registry::delete(Hive::Hkcu, TEST_PATH, name);

        let action = Action::SetReg {
            hive: Hive::Hkcu,
            path: TEST_PATH.into(),
            name: name.into(),
            value: RegValue::Sz("hello".into()),
        };
        let rev = apply(&action).unwrap();
        assert_eq!(
            registry::read(Hive::Hkcu, TEST_PATH, name).unwrap(),
            Some(RegValue::Sz("hello".into()))
        );

        revert(&rev).unwrap();
        assert_eq!(registry::read(Hive::Hkcu, TEST_PATH, name).unwrap(), None);
    }

    #[test]
    fn overwrite_existing_value_then_revert_restores_original() {
        let name = "EngineOverwrite";
        registry::write(Hive::Hkcu, TEST_PATH, name, &RegValue::Dword(1)).unwrap();

        let action = Action::SetReg {
            hive: Hive::Hkcu,
            path: TEST_PATH.into(),
            name: name.into(),
            value: RegValue::Dword(99),
        };
        let rev = apply(&action).unwrap();
        assert_eq!(
            registry::read(Hive::Hkcu, TEST_PATH, name).unwrap(),
            Some(RegValue::Dword(99))
        );

        revert(&rev).unwrap();
        assert_eq!(
            registry::read(Hive::Hkcu, TEST_PATH, name).unwrap(),
            Some(RegValue::Dword(1))
        );

        let _ = registry::delete(Hive::Hkcu, TEST_PATH, name);
    }
}
