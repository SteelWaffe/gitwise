//! Tokens and SSH passphrases live in the OS credential store (Windows Credential Manager,
//! macOS Keychain), never in the app's JSON files.

use crate::error::Result;

const SERVICE: &str = "gitwise";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Token,
    SshPassphrase,
}

fn entry(profile_id: &str, kind: SecretKind) -> Result<keyring::Entry> {
    let k = match kind {
        SecretKind::Token => "token",
        SecretKind::SshPassphrase => "ssh-passphrase",
    };
    Ok(keyring::Entry::new(SERVICE, &format!("profile:{profile_id}:{k}"))?)
}

pub fn get(profile_id: &str, kind: SecretKind) -> Result<Option<String>> {
    match entry(profile_id, kind)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set(profile_id: &str, kind: SecretKind, value: &str) -> Result<()> {
    Ok(entry(profile_id, kind)?.set_password(value)?)
}

pub fn delete(profile_id: &str, kind: SecretKind) -> Result<()> {
    match entry(profile_id, kind)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_all(profile_id: &str) -> Result<()> {
    delete(profile_id, SecretKind::Token)?;
    delete(profile_id, SecretKind::SshPassphrase)
}
