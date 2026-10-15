use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        // macOS /var is itself a symlink; tests use its real path like the home path.
        let path = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("shvia-cli-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn destination(&self) -> PathBuf {
        self.0.join("settings.json")
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn replacement_does_not_create_a_credential_backup_or_touch_an_old_one() {
    let fixture = Fixture::new();
    let path = fixture.destination();
    std::fs::write(&path, "previous-secret").unwrap();
    let legacy_backup = fixture.0.join("settings.shvia-bak");
    std::fs::write(&legacy_backup, "legacy-backup").unwrap();

    write(&path, Some("previous-secret"), "current-secret", false).unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "current-secret");
    assert_eq!(
        std::fs::read_to_string(legacy_backup).unwrap(),
        "legacy-backup"
    );
    assert_eq!(fixture.names(), ["settings.json", "settings.shvia-bak"]);
}

#[test]
fn partial_write_failure_keeps_the_original_and_removes_the_temporary() {
    let fixture = Fixture::new();
    let path = fixture.destination();
    std::fs::write(&path, "previous-secret").unwrap();

    let result = replace_with(&path, Some("previous-secret"), false, |file| {
        file.write_all(b"partial-new-secret")?;
        Err(io::Error::other("injected disk failure"))
    });

    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous-secret");
    assert_eq!(fixture.names(), ["settings.json"]);
}

#[test]
fn failed_initial_write_leaves_no_credential_file() {
    let fixture = Fixture::new();
    let result = replace_with(&fixture.destination(), None, false, |file| {
        file.write_all(b"partial-secret")?;
        Err(io::Error::other("injected disk failure"))
    });
    assert!(result.is_err());
    assert!(fixture.names().is_empty());
}

#[test]
fn changes_during_consent_are_preserved() {
    let fixture = Fixture::new();
    let path = fixture.destination();
    std::fs::write(&path, "before-consent").unwrap();
    let previous = read(&path).unwrap();
    std::fs::write(&path, "changed-during-consent").unwrap();

    assert!(write(&path, previous.as_deref(), "replacement", false).is_err());
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "changed-during-consent"
    );
    assert_eq!(fixture.names(), ["settings.json"]);
}

#[test]
fn a_file_created_during_consent_is_not_overwritten() {
    let fixture = Fixture::new();
    let path = fixture.destination();
    assert!(read(&path).unwrap().is_none());
    std::fs::write(&path, "created-by-another-client").unwrap();
    assert!(write(&path, None, "replacement", false).is_err());
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "created-by-another-client"
    );
}

#[test]
fn a_change_while_staging_aborts_and_cleans_the_temporary() {
    let fixture = Fixture::new();
    let path = fixture.destination();
    std::fs::write(&path, "original").unwrap();
    let result = replace_with(&path, Some("original"), false, |file| {
        file.write_all(b"replacement")?;
        std::fs::write(&path, "concurrent-edit")
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "concurrent-edit");
    assert_eq!(fixture.names(), ["settings.json"]);
}

#[test]
fn non_regular_or_invalid_utf8_configurations_are_errors() {
    let fixture = Fixture::new();
    let path = fixture.destination();
    std::fs::create_dir(&path).unwrap();
    assert!(read(&path).is_err());
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    assert!(read(&path).is_err());
    assert!(write(&path, None, "replacement", false).is_err());
    assert_eq!(std::fs::read(path).unwrap(), [0xff, 0xfe]);
}

#[test]
fn only_the_owned_final_directory_can_be_created() {
    let fixture = Fixture::new();
    let path = fixture.0.join(".shvia").join("env.sh");
    assert!(read(&path).unwrap().is_none());
    assert!(write(&path, None, "secret", false).is_err());
    assert!(!path.parent().unwrap().exists());
    write(&path, None, "secret", true).unwrap();
    assert_eq!(read(&path).unwrap().as_deref(), Some("secret"));
    assert!(write(
        &fixture.0.join("missing/parent/env.sh"),
        None,
        "secret",
        true
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn existing_broad_permissions_are_not_inherited_by_the_replacement() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let path = fixture.destination();
    std::fs::write(&path, "original").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    replace_with(&path, Some("original"), false, |file| {
        assert_eq!(file.metadata()?.permissions().mode() & 0o777, 0o600);
        file.write_all(b"replacement")
    })
    .unwrap();
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
}

#[cfg(target_os = "macos")]
#[test]
fn inherited_macos_acl_is_absent_before_any_credential_bytes_are_written() {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn acl_get_fd_np(fd: libc::c_int, kind: libc::c_int) -> *mut libc::c_void;
        fn acl_get_entry(
            acl: *mut libc::c_void,
            index: libc::c_int,
            entry: *mut *mut libc::c_void,
        ) -> libc::c_int;
        fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
    }
    fn has_acl_entry(file: &File) -> bool {
        // SAFETY: the descriptor is valid, the allocated ACL is released once.
        unsafe {
            let acl = acl_get_fd_np(file.as_raw_fd(), 0x100);
            if acl.is_null() {
                // APFS reports ENOENT when the open file has no extended ACL.
                assert_eq!(
                    io::Error::last_os_error().raw_os_error(),
                    Some(libc::ENOENT)
                );
                return false;
            }
            let mut entry = std::ptr::null_mut();
            let present = acl_get_entry(acl, 0, &mut entry) == 0;
            acl_free(acl);
            present
        }
    }

    let fixture = Fixture::new();
    let status = crate::processo::comando("/bin/chmod")
        .args(["+a", "everyone allow read,file_inherit,directory_inherit"])
        .arg(&fixture.0)
        .status()
        .unwrap();
    assert!(status.success());
    // Prove this filesystem really supplies the inherited permission we are testing.
    let control = fixture.0.join("control");
    std::fs::write(&control, "").unwrap();
    assert!(has_acl_entry(&File::open(&control).unwrap()));
    std::fs::remove_file(control).unwrap();

    let path = fixture.destination();
    replace_with(&path, None, false, |file| {
        assert!(
            !has_acl_entry(file),
            "the empty credential file is already private"
        );
        file.write_all(b"test-credential")
    })
    .unwrap();
    assert!(!has_acl_entry(&File::open(path).unwrap()));
    assert_eq!(fixture.names(), ["settings.json"]);
}

#[cfg(windows)]
#[test]
fn windows_dacl_is_private_before_writing_and_remains_private_after_replacement() {
    use std::os::windows::io::AsRawHandle;
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{LocalFree, HANDLE, HLOCAL},
            Security::{
                Authorization::{
                    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo,
                    SDDL_REVISION_1, SE_FILE_OBJECT,
                },
                DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
            },
        },
    };

    fn assert_private(file: &File) {
        // SAFETY: GetSecurityInfo returns an owned descriptor and the conversion
        // returns an owned string; both are freed after copying the string.
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            GetSecurityInfo(
                HANDLE(file.as_raw_handle()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                None,
                None,
                Some(&mut descriptor),
            )
            .ok()
            .unwrap();
            let mut text = PWSTR::null();
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                None,
            )
            .unwrap();
            let text_value = text.to_string().unwrap();
            LocalFree(Some(HLOCAL(text.0.cast())));
            LocalFree(Some(HLOCAL(descriptor.0)));
            assert!(
                text_value.starts_with("D:P"),
                "protected DACL required: {text_value}"
            );
            assert!(
                text_value.contains("(A;;FA;;;OW)"),
                "owner access required: {text_value}"
            );
            assert!(
                text_value.contains("(A;;FA;;;SY)"),
                "SYSTEM access required: {text_value}"
            );
            assert_eq!(
                text_value.matches('(').count(),
                2,
                "no inherited or public ACE: {text_value}"
            );
        }
    }

    let fixture = Fixture::new();
    let path = fixture.destination();
    std::fs::write(&path, "original").unwrap();
    replace_with(&path, Some("original"), false, |file| {
        assert_private(file);
        file.write_all(b"test-credential")
    })
    .unwrap();
    assert_private(&File::open(path).unwrap());
}

#[cfg(unix)]
#[test]
fn destination_and_parent_symlinks_are_rejected_without_touching_the_target() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let outside = Fixture::new();
    let target = outside.destination();
    std::fs::write(&target, "outside-secret").unwrap();
    let path = fixture.destination();
    symlink(&target, &path).unwrap();
    assert!(read(&path).is_err());
    assert!(write(&path, None, "replacement", false).is_err());
    std::fs::remove_file(&path).unwrap();
    let linked_parent = fixture.0.join("linked");
    symlink(&outside.0, &linked_parent).unwrap();
    assert!(read(&linked_parent.join("settings.json")).is_err());
    assert!(write(
        &linked_parent.join("settings.json"),
        None,
        "replacement",
        false
    )
    .is_err());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "outside-secret");
    assert_eq!(outside.names(), ["settings.json"]);
}

#[cfg(unix)]
#[test]
fn unreadable_configuration_is_not_treated_as_missing() {
    use std::os::unix::fs::PermissionsExt;
    // A privileged CI runner can bypass file access permissions.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let fixture = Fixture::new();
    let path = fixture.destination();
    std::fs::write(&path, "original").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    assert!(read(&path).is_err());
    assert!(write(&path, None, "replacement", false).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
    assert_eq!(fixture.names(), ["settings.json"]);
}
