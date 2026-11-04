// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Write a complete drop beside its destination and publish it with rename.
/// Validation, write, flush, and sync failures leave the old destination alone.
/// Platforms that cannot rename over an existing destination return that
/// publication error with the old file intact. On Unix, existing permission
/// bits are retained; new files use the usual `0666` mode filtered by umask.
pub(super) fn write_staged_drop<T>(
    target: &Path,
    write: impl FnOnce(&mut BufWriter<File>) -> Result<T, NativeDropError>,
) -> Result<T, NativeDropError> {
    let (temporary, file, existing_permissions) = create_export_temporary(target)?;
    let result = (|| {
        let mut writer = BufWriter::new(file);
        let result = write(&mut writer)?;
        writer.flush()?;
        if let Some(permissions) = existing_permissions {
            writer.get_ref().set_permissions(permissions)?;
        }
        writer.get_ref().sync_all()?;
        drop(writer);
        fs::rename(&temporary, target)?;
        Ok(result)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(super) fn create_export_temporary(
    target: &Path,
) -> std::io::Result<(PathBuf, File, Option<std::fs::Permissions>)> {
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = target.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "drop export path has no file name",
        )
    })?;
    let existing_permissions = match fs::metadata(target) {
        Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
        Ok(_) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };

    loop {
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT_EXPORT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = parent.join(temporary_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let mode = existing_permissions
                .as_ref()
                .map(|permissions| (permissions.mode() & 0o7777) | 0o200)
                .unwrap_or(0o666);
            options.mode(mode);
        }
        match options.open(&temporary) {
            Ok(file) => {
                return Ok((temporary, file, existing_permissions));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Export one topic to a plaintext file for public data or explicit local use.
pub async fn export_plain_topic_file<B, E, L>(
    path: impl AsRef<Path>,
    store: &MunimentStore<B, E>,
    topic: &Topic,
    profile: DropExportProfile,
    limits: DropLimits,
) -> Result<DropWriteReceipt, DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
{
    let records = export_topic_operations::<B, E, L>(store, topic, profile).await?;
    Ok(write_staged_drop(path.as_ref(), |writer| {
        write_plain_drop(writer, &records, limits)
    })?)
}

/// Export one topic after domain privacy/priority selection.
pub async fn export_selected_plain_topic_file<B, E, L, S>(
    path: impl AsRef<Path>,
    store: &MunimentStore<B, E>,
    topic: &Topic,
    selector: &S,
    budget: DropExportBudget,
    limits: DropLimits,
) -> Result<DropFileExportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
    S: DropExportSelector<E>,
{
    let (records, selection) =
        export_topic_operations_selected::<B, E, L, S>(store, topic, selector, budget).await?;
    let drop = write_staged_drop(path.as_ref(), |writer| {
        write_plain_drop(writer, &records, limits)
    })?;
    Ok(DropFileExportReport { drop, selection })
}

/// Export private data through domain selection and an injected protection
/// suite. Private export has no unfiltered convenience path.
pub async fn export_protected_topic_file<B, E, L, S, D>(
    path: impl AsRef<Path>,
    store: &MunimentStore<B, E>,
    topic: &Topic,
    selector: &S,
    budget: DropExportBudget,
    limits: DropLimits,
    protector: &D,
) -> Result<DropFileExportReport, DropIoError>
where
    B: Backend,
    E: Extensions,
    L: LogId,
    S: DropExportSelector<E>,
    D: DropProtector,
{
    let (records, selection) =
        export_topic_operations_selected::<B, E, L, S>(store, topic, selector, budget).await?;
    let drop = write_staged_drop(path.as_ref(), |writer| {
        write_protected_drop(writer, &records, limits, protector)
    })?;
    Ok(DropFileExportReport { drop, selection })
}
