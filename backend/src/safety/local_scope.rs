//! Lexical containment before inspecting a caller-supplied local path. Callers
//! still check reparse points and canonical containment before accessing data.
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
pub(crate) fn relative(root: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(root).ok().map(Path::to_path_buf)
}

#[cfg(windows)]
pub(crate) fn relative(root: &Path, path: &Path) -> Option<PathBuf> {
    use std::path::{Component, Prefix};
    let local_disk = |p: &Path| {
        p.is_absolute()
            && matches!(p.components().next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
    };
    if !local_disk(root)
        || !local_disk(path)
        || path.components().any(|c| c == Component::ParentDir)
    {
        return None;
    }
    relative_with_alias(root, path, None).or_else(|| {
        // Ask Windows about the existing trusted root only. A short spelling
        // in TEMP (e.g. RUNNER~1) can coexist with long child directory names.
        let short = short_name(root)?;
        relative_with_alias(root, path, Some(&short))
    })
}

#[cfg(windows)]
fn relative_with_alias(root: &Path, path: &Path, short: Option<&Path>) -> Option<PathBuf> {
    use std::path::{Component, Prefix};
    fn same(a: Component<'_>, b: Component<'_>) -> bool {
        match (a, b) {
            (Component::Prefix(a), Component::Prefix(b)) => {
                matches!((a.kind(),b.kind()), (Prefix::Disk(x) | Prefix::VerbatimDisk(x), Prefix::Disk(y) | Prefix::VerbatimDisk(y)) if x.eq_ignore_ascii_case(&y))
            }
            (Component::Normal(a), Component::Normal(b)) => a.eq_ignore_ascii_case(b),
            (Component::RootDir, Component::RootDir) => true,
            _ => false,
        }
    }
    let mut names = path.components();
    let mut aliases = short.map(Path::components);
    for component in root.components() {
        let input = names.next()?;
        let alias = aliases.as_mut().and_then(Iterator::next);
        if !same(component, input) && !alias.is_some_and(|a| same(a, input)) {
            return None;
        }
    }
    Some(names.as_path().to_owned())
}

#[cfg(windows)]
fn short_name(root: &Path) -> Option<PathBuf> {
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
    };
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    let mut input: Vec<_> = root.as_os_str().encode_wide().collect();
    if input.contains(&0) {
        return None;
    }
    input.push(0);
    // Both buffers use UTF-16. The size query includes the terminating NUL.
    let size = unsafe { GetShortPathNameW(input.as_ptr(), std::ptr::null_mut(), 0) };
    if size == 0 {
        return None;
    }
    let mut buffer = vec![0; size as usize];
    let used = unsafe { GetShortPathNameW(input.as_ptr(), buffer.as_mut_ptr(), size) };
    if used == 0 || used >= size {
        return None;
    }
    Some(OsString::from_wide(&buffer[..used as usize]).into())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn mixed_short_and_long_root_components_preserve_the_relative_name() {
        let root = Path::new(r"\\?\C:\Users\Runner Administrator\AppData\Local\Temp\Project Files");
        let alias = Path::new(r"C:\Users\RUNNER~1\AppData\Local\Temp\PROJEC~1");
        let request = Path::new(r"C:\Users\RUNNER~1\AppData\Local\Temp\Project Files\Notes.txt");
        assert_eq!(
            relative_with_alias(root, request, Some(alias)),
            Some(PathBuf::from("Notes.txt"))
        );
        assert_eq!(
            relative_with_alias(
                root,
                Path::new(r"C:\Users\RUNNER~2\AppData\Local\Temp\Project Files"),
                Some(alias)
            ),
            None
        );
    }
}
