//! Credential-bearing CLI files are replaced without retaining an old copy.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Keep the consent-time contents in memory only. Unreadable files are errors.
pub(super) fn read(path: &Path) -> io::Result<Option<String>> {
    match Directory::open(path.parent().ok_or_else(invalid_path)?, false) {
        Ok(directory) => directory.read(path.file_name().ok_or_else(invalid_path)?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) fn write(
    path: &Path,
    previous: Option<&str>,
    content: &str,
    create_parent: bool,
) -> io::Result<()> {
    replace_with(path, previous, create_parent, |file| {
        file.write_all(content.as_bytes())
    })
}

fn replace_with(
    path: &Path,
    previous: Option<&str>,
    create_parent: bool,
    write_content: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let directory = Directory::open(path.parent().ok_or_else(invalid_path)?, create_parent)?;
    let name = path.file_name().ok_or_else(invalid_path)?;
    if directory.read(name)?.as_deref() != previous {
        return Err(io::Error::other(
            "a configuração mudou; abra a confirmação novamente",
        ));
    }

    let temporary = PathBuf::from(format!(".shvia-cli-{}.tmp", uuid::Uuid::new_v4()));
    let mut file = directory.create(&temporary)?;
    let staged = (|| {
        write_content(&mut file)?;
        file.sync_all()?;
        // Windows cannot rename the file while its exclusive creation handle is open.
        drop(file);
        if directory.read(name)?.as_deref() != previous {
            return Err(io::Error::other(
                "a configuração mudou; abra a confirmação novamente",
            ));
        }
        directory.replace(&temporary, name)
    })();
    // Only our new temporary is removed. Existing .shvia-bak files are never touched.
    if staged.is_err() {
        let _ = directory.remove(&temporary);
    }
    staged
}

fn invalid_path() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "caminho de configuração inválido",
    )
}

#[cfg(unix)]
struct Directory(File);

#[cfg(unix)]
impl Directory {
    fn open(path: &Path, create: bool) -> io::Result<Self> {
        use std::os::fd::{AsRawFd, FromRawFd};
        use std::os::unix::fs::OpenOptionsExt;
        use std::path::Component;

        if !path.is_absolute() {
            return Err(invalid_path());
        }
        let mut directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")?;
        let mut components = path
            .components()
            .filter(|part| *part != Component::RootDir)
            .peekable();
        while let Some(component) = components.next() {
            let Component::Normal(name) = component else {
                return Err(invalid_path());
            };
            let name = c_name(name)?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            // SAFETY: the directory descriptor and NUL-terminated name remain valid.
            let mut fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0
                && io::Error::last_os_error().kind() == io::ErrorKind::NotFound
                && create
                && components.peek().is_none()
            {
                // Only the final directory (~/.shvia) may be created, with no secret in it.
                // SAFETY: the same live descriptor and name are used by mkdirat/openat.
                let created = unsafe { libc::mkdirat(directory.as_raw_fd(), name.as_ptr(), 0o700) };
                if created < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists
                {
                    return Err(io::Error::last_os_error());
                }
                fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
            }
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: openat returned a new descriptor, now owned by File.
            directory = unsafe { File::from_raw_fd(fd) };
        }
        Ok(Self(directory))
    }

    fn read(&self, name: &std::ffi::OsStr) -> io::Result<Option<String>> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let name = c_name(name)?;
        // O_NONBLOCK avoids hanging on a FIFO; metadata then rejects non-regular files.
        // SAFETY: the descriptor and name are valid for this openat call.
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(error)
            };
        }
        // SAFETY: the newly opened descriptor transfers to File exactly once.
        read_regular(unsafe { File::from_raw_fd(fd) }).map(Some)
    }

    #[cfg(not(target_os = "macos"))]
    fn create(&self, name: &Path) -> io::Result<File> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let c_name = c_name(name.as_os_str())?;
        // Exclusive creation never follows a preexisting link. No secret is written yet.
        // SAFETY: openat receives a valid descriptor/name and the required mode argument.
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                c_name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: ownership of the new descriptor passes to File.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    #[cfg(target_os = "macos")]
    fn create(&self, name: &Path) -> io::Result<File> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let name_c = c_name(name.as_os_str())?;
        // An inherited macOS ACL may bypass mode 0600. Start with an EMPTY directory:
        // after its ACL is removed, files created inside cannot inherit broad access.
        // Even a descriptor opened to the empty directory before revocation grants no
        // access to the later 0600 credential file, which did not exist at that time.
        // SAFETY: mkdirat/openat use a live parent descriptor and NUL-terminated name.
        if unsafe { libc::mkdirat(self.0.as_raw_fd(), name_c.as_ptr(), 0o700) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let result = (|| {
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    name_c.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: the new descriptor is owned by this File only.
            let staging = unsafe { File::from_raw_fd(fd) };
            remove_inherited_acl(&staging)?;
            let child = c_name(std::ffi::OsStr::new("config"))?;
            // SAFETY: the private staging directory is held open and child is valid.
            let fd = unsafe {
                libc::openat(
                    staging.as_raw_fd(),
                    child.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(unsafe { File::from_raw_fd(fd) })
            }
        })();
        if result.is_err() {
            let _ = self.remove(name);
        }
        result
    }

    fn replace(&self, from: &Path, to: &std::ffi::OsStr) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        #[cfg(target_os = "macos")]
        let file = from.join("config");
        #[cfg(not(target_os = "macos"))]
        let file = from.to_path_buf();
        #[cfg(target_os = "macos")]
        let staging_name = c_name(from.as_os_str())?;
        let file = c_name(file.as_os_str())?;
        let to = c_name(to)?;
        // The directory fd anchors both names even if an ancestor is renamed meanwhile.
        // SAFETY: both names and the directory descriptor live throughout renameat.
        let result = unsafe {
            libc::renameat(
                self.0.as_raw_fd(),
                file.as_ptr(),
                self.0.as_raw_fd(),
                to.as_ptr(),
            )
        };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            // The credential file has committed. Removing an empty staging directory
            // cannot change success into failure and never deletes a replacement file.
            #[cfg(target_os = "macos")]
            {
                unsafe {
                    libc::unlinkat(
                        self.0.as_raw_fd(),
                        staging_name.as_ptr(),
                        libc::AT_REMOVEDIR,
                    )
                };
            }
            Ok(())
        }
    }

    fn remove(&self, name: &Path) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        #[cfg(target_os = "macos")]
        {
            let child = c_name(name.join("config").as_os_str())?;
            unsafe { libc::unlinkat(self.0.as_raw_fd(), child.as_ptr(), 0) };
        }
        let name = c_name(name.as_os_str())?;
        #[cfg(target_os = "macos")]
        let flags = libc::AT_REMOVEDIR;
        #[cfg(not(target_os = "macos"))]
        let flags = 0;
        // SAFETY: unlinkat receives a live descriptor and NUL-terminated file name.
        if unsafe { libc::unlinkat(self.0.as_raw_fd(), name.as_ptr(), flags) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(unix)]
fn c_name(name: &std::ffi::OsStr) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(name.as_bytes()).map_err(|_| invalid_path())
}

#[cfg(target_os = "macos")]
fn remove_inherited_acl(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn acl_init(count: libc::c_int) -> *mut libc::c_void;
        fn acl_set_fd_np(fd: libc::c_int, acl: *mut libc::c_void, kind: libc::c_int)
            -> libc::c_int;
        fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
    }
    // macOS extended ACLs can grant access independently of mode 0600. Remove inherited
    // entries while the new file is still empty; failure prevents writing any secret.
    // SAFETY: acl_init returns an ACL owned here, passed to acl_set_fd_np and freed once.
    unsafe {
        let acl = acl_init(0);
        if acl.is_null() {
            return Err(io::Error::last_os_error());
        }
        let result = acl_set_fd_np(file.as_raw_fd(), acl, 0x100); // ACL_TYPE_EXTENDED
        let error = io::Error::last_os_error();
        acl_free(acl);
        if result < 0 {
            Err(error)
        } else {
            Ok(())
        }
    }
}

fn read_regular(mut file: File) -> io::Result<String> {
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a configuração precisa ser um arquivo regular",
        ));
    }
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}

#[cfg(windows)]
struct Directory {
    path: PathBuf,
    // No FILE_SHARE_DELETE: pin every ancestor against rename/junction replacement.
    _handles: Vec<File>,
}

#[cfg(windows)]
impl Directory {
    fn open(path: &Path, create: bool) -> io::Result<Self> {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        if !path.is_absolute() {
            return Err(invalid_path());
        }
        let mut handles = Vec::new();
        let ancestors: Vec<_> = path.ancestors().collect();
        for ancestor in ancestors
            .iter()
            .rev()
            .filter(|part| !part.as_os_str().is_empty())
        {
            let opened = || {
                std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
                    .custom_flags((FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).0)
                    .open(ancestor)
            };
            let file = match opened() {
                Err(error)
                    if create && *ancestor == path && error.kind() == io::ErrorKind::NotFound =>
                {
                    std::fs::create_dir(path)?;
                    opened()?
                }
                result => result?,
            };
            let metadata = file.metadata()?;
            if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
                return Err(invalid_path()); // FILE_ATTRIBUTE_REPARSE_POINT (includes junctions)
            }
            handles.push(file);
        }
        Ok(Self {
            path: path.into(),
            _handles: handles,
        })
    }

    fn read(&self, name: &std::ffi::OsStr) -> io::Result<Option<String>> {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(self.path.join(name))
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if file.metadata()?.file_attributes() & 0x400 != 0 {
            return Err(invalid_path());
        }
        read_regular(file).map(Some)
    }

    fn create(&self, name: &Path) -> io::Result<File> {
        use std::os::windows::{ffi::OsStrExt, io::FromRawHandle};
        use windows::{
            core::{w, PCWSTR},
            Win32::{
                Foundation::{LocalFree, GENERIC_WRITE, HLOCAL},
                Security::{
                    Authorization::{
                        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
                    },
                    PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
                },
                Storage::FileSystem::{
                    CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE,
                },
            },
        };
        let path: Vec<u16> = self
            .path
            .join(name)
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // A protected DACL excludes inherited access. Only the owner and SYSTEM have access.
        // SAFETY: the descriptor lives until CreateFileW returns, then LocalFree releases it.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                w!("D:P(A;;FA;;;OW)(A;;FA;;;SY)"),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
            .map_err(io::Error::from)?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            let handle = CreateFileW(
                PCWSTR(path.as_ptr()),
                GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                Some(&attributes),
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                None,
            );
            LocalFree(Some(HLOCAL(descriptor.0)));
            handle
                .map(|handle| File::from_raw_handle(handle.0))
                .map_err(io::Error::from)
        }
    }

    fn replace(&self, from: &Path, to: &std::ffi::OsStr) -> io::Result<()> {
        // std::fs::rename uses MoveFileExW(MOVEFILE_REPLACE_EXISTING), preserving the
        // newly created file's protected DACL rather than the previous destination ACL.
        std::fs::rename(self.path.join(from), self.path.join(to))
    }

    fn remove(&self, name: &Path) -> io::Result<()> {
        std::fs::remove_file(self.path.join(name))
    }
}

#[cfg(test)]
mod tests;
