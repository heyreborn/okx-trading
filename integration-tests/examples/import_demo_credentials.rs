//! One-time local migration of three OKX demo secrets into protected files.
//! The source dotenv file is never used by runtime modules or live tests.

use std::collections::BTreeMap;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

fn import(source: &Path, destination: &Path) -> Result<(), &'static str> {
    let metadata = fs::symlink_metadata(source).map_err(|_| "cannot inspect source")?;
    if !metadata.file_type().is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("source must be a private regular file");
    }
    let mut values = BTreeMap::new();
    let entries = dotenvy::from_path_iter(source).map_err(|_| "cannot read source")?;
    for entry in entries {
        let (name, value) = entry.map_err(|_| "cannot parse source")?;
        if matches!(
            name.as_str(),
            "OKX_API_KEY" | "OKX_API_SECRET" | "OKX_API_PASSPHRASE"
        ) && values.insert(name, value).is_some()
        {
            return Err("duplicate credential field");
        }
    }
    let selected = [
        ("OKX_API_KEY", "api-key.secret"),
        ("OKX_API_SECRET", "api-secret.secret"),
        ("OKX_API_PASSPHRASE", "api-passphrase.secret"),
    ];
    if selected
        .iter()
        .any(|(name, _)| values.get(*name).is_none_or(String::is_empty))
    {
        return Err("missing credential field");
    }
    let mut directory = DirBuilder::new();
    directory.mode(0o700);
    directory
        .create(destination)
        .map_err(|_| "destination must not exist")?;
    for (name, filename) in selected {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination.join(filename))
            .map_err(|_| "cannot create credential file")?;
        file.write_all(values.get(name).expect("checked field").as_bytes())
            .map_err(|_| "cannot write credential file")?;
        file.sync_all().map_err(|_| "cannot sync credential file")?;
    }
    Ok(())
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let source = args.next().expect("source dotenv path required");
    let destination = args.next().expect("destination directory required");
    assert!(args.next().is_none(), "unexpected argument");
    import(Path::new(&source), Path::new(&destination)).expect("credential import failed");
}
